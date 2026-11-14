#[path = "support/input_patterns.rs"]
mod input_patterns;

use carina_core::protocol_convert::{protocol_to_resource_schema, resource_schema_to_protocol};
use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{EffectiveType, RawShape, ValidatedResourceSchema};
use carina_provider_aws::schemas::generated::ec2::{
    eip::ec2_eip_config, flow_log::ec2_flow_log_config, security_group::ec2_security_group_config,
    subnet::ec2_subnet_config, transit_gateway::ec2_transit_gateway_config, vpc::ec2_vpc_config,
    vpn_gateway::ec2_vpn_gateway_config,
};

fn schema(resource: &str) -> ValidatedResourceSchema {
    let config = match resource {
        "Eip" => ec2_eip_config(),
        "FlowLog" => ec2_flow_log_config(),
        "SecurityGroup" => ec2_security_group_config(),
        "Subnet" => ec2_subnet_config(),
        "TransitGateway" => ec2_transit_gateway_config(),
        "Vpc" => ec2_vpc_config(),
        "VpnGateway" => ec2_vpn_gateway_config(),
        _ => panic!("unknown EC2 resource {resource}"),
    };
    config.schema.validate().expect("EC2 schema admission")
}

fn field<'a>(schema: &'a ValidatedResourceSchema, path: &str) -> EffectiveType<'a> {
    let names = path
        .split('.')
        .map(|name| name.trim_end_matches("[]").to_string())
        .collect::<Vec<_>>();
    let ty = schema.effective_node_at_path(&names).unwrap().value_type();
    if path.ends_with("[]") {
        ty.list_element().unwrap()
    } else {
        ty
    }
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}

fn int(value: i64) -> Value {
    Value::Concrete(ConcreteValue::Int(value))
}

fn strings(resource: &str, path: &str, valid: &[&str], invalid: &[&str]) {
    let schema = schema(resource);
    let ty = field(&schema, path);
    for (values, expected) in [(valid, true), (invalid, false)] {
        for value in values {
            assert_eq!(
                ty.validate(&string(value)).is_ok() && input_patterns::accept(&schema, path, value),
                expected,
                "{resource}.{path} = {value:?}"
            );
        }
    }
    assert!(ty.validate(&int(1)).is_err());
}

#[test]
fn ec2_eip_address_is_ipv4() {
    // ec2.json AllocateAddressRequest$Address -> PublicIpAddress; API_AllocateAddress.
    strings(
        "Eip",
        "address",
        &["198.51.100.10", "0.0.0.0", "255.255.255.255"],
        &["", "2001:db8::1", "198.51.100.10/32", "256.1.1.1", "1.2.3"],
    );
}

#[test]
fn ec2_flow_log_format_requires_space_separated_tokens_without_freezing_fields() {
    // CreateFlowLogsRequest$LogFormat and both linked flow-log field catalogs.
    strings(
        "FlowLog",
        "log_format",
        &[
            "${version}",
            "${srcaddr} ${dstaddr}",
            "${tgw-id} ${packets-lost-no-route}",
            "${instance-tag-2} ${next-hop-interface-id}",
            "${future-field-42}",
        ],
        &[
            "",
            " ",
            "version",
            "${}",
            "${version}${bytes}",
            "${version},${bytes}",
            "${version}\n${bytes}",
            "${version}\t${bytes}",
            "${version} trailing",
        ],
    );
}

#[test]
fn ec2_flow_log_group_accepts_existing_aws_groups_and_checks_cloudwatch_name_bounds() {
    strings(
        "FlowLog",
        "log_group_name",
        &[
            "/aws/vpc/flow-logs",
            "aws/existing-group",
            ".-_/#AZaz09",
            &"a".repeat(512),
        ],
        &["", "has space", "bad:name", "valid\n", &"a".repeat(513)],
    );
    let schema = schema("FlowLog");
    let RawShape::String { identity_layer, .. } =
        field(&schema, "log_group_name").as_type().raw_shape()
    else {
        panic!("log group name");
    };
    assert_eq!(
        identity_layer.identity.unwrap().to_string(),
        "aws.logs.LogGroup.LogGroupName"
    );
}

#[test]
fn ec2_flow_log_aggregation_interval_is_exactly_60_or_600() {
    let schema = schema("FlowLog");
    let ty = field(&schema, "max_aggregation_interval");
    for value in [-1, 0, 1, 59, 60, 61, 599, 600, 601, i64::MAX] {
        assert_eq!(
            ty.validate(&int(value)).is_ok(),
            matches!(value, 60 | 600),
            "{value}"
        );
    }
    assert!(ty.validate(&string("60")).is_err());
}

#[test]
fn ec2_flow_log_selected_tag_keys_have_smithy_bounds_without_a_new_alphabet() {
    strings(
        "FlowLog",
        "tag_field_specifications[].tag_keys[]",
        &[
            "Name",
            "aws:cloudformation:stack-name",
            "日本語",
            &"é".repeat(128),
        ],
        &["", &"a".repeat(129), &"é".repeat(129)],
    );
}

#[test]
fn ec2_flow_log_tag_lists_keep_smithy_cardinality_in_native_and_protocol_schemas() {
    let native = schema("FlowLog");
    let wire = resource_schema_to_protocol(&native).unwrap();
    let decoded = protocol_to_resource_schema(&wire).unwrap();
    for schema in [native, decoded] {
        for (path, maximum) in [
            ("tag_field_specifications", 3),
            ("tag_field_specifications[].tag_keys", 2),
        ] {
            let RawShape::List { constraints, .. } = field(&schema, path).as_type().raw_shape()
            else {
                panic!("{path} must remain a List");
            };
            assert_eq!(
                constraints.length(),
                Some((Some(1), Some(maximum))),
                "{path}"
            );
        }
    }
}

#[test]
fn ec2_security_group_name_enforces_documented_alphabet_prefix_and_length() {
    // carina-test-004, ap-northeast-1, 2026-11-05: real CreateSecurityGroup
    // rejects sg-probe and non-ASCII; SG-Probe and the full ASCII sample pass.
    // The reserved lowercase prefix is case-sensitive. DryRun validates none
    // of these cases; it is not evidence of acceptance.
    strings(
        "SecurityGroup",
        "group_name",
        &[
            "default",
            "SG-Probe",
            "probe ok._-:/()#,@[]+=&;{}!$*",
            "carina-registry-publish-api-service-sg",
            "AZaz09 ._-:/()#,@[]+=&;{}!$*",
            &"a".repeat(255),
        ],
        &[
            "",
            "sg-forbidden",
            "sg-probe",
            "sg-",
            "bad?name",
            "bad\\name",
            "日本語",
            "プローブ",
            "valid\n",
            &"a".repeat(256),
        ],
    );
}

#[test]
fn ec2_vpc_ipv4_cidr_checks_prefix_and_documented_exclusions() {
    // carina-test-004, 2026-11-05: CreateVpc rejects /15, /29, and
    // 0.0/16, 127.0/16, 169.254/16, 224.0/16 with InvalidVpc.Range.
    strings(
        "Vpc",
        "cidr_block",
        &[
            "10.0.0.0/16",
            "10.0.0.0/28",
            "100.68.0.18/18",
            "8.8.0.0/16",
            "172.17.0.0/16",
            "240.0.0.0/16",
        ],
        &[
            "10.0.0.0/15",
            "10.0.0.0/29",
            "0.1.0.0/16",
            "0.0.0.0/16",
            "127.1.0.0/16",
            "127.0.0.0/16",
            "169.254.0.0/16",
            "224.1.0.0/16",
            "224.0.0.0/16",
            "239.255.0.0/16",
            "256.0.0.0/16",
            "2001:db8::/64",
        ],
    );
}

#[test]
fn ec2_subnet_ipv4_cidr_checks_resource_specific_prefix_without_requiring_canonical_input() {
    // carina-test-004, 2026-11-05: CreateSubnet rejects /29 and accepts /28.
    strings(
        "Subnet",
        "cidr_block",
        &["10.0.0.0/16", "10.0.0.0/28", "100.68.0.18/18"],
        &[
            "10.0.0.0/15",
            "10.0.0.0/29",
            "10.0.0.0/32",
            "256.0.0.0/24",
            "2001:db8::/64",
        ],
    );
}

#[test]
fn ec2_subnet_ipv6_cidr_allows_every_documented_prefix_and_existing_read_spelling() {
    strings(
        "Subnet",
        "ipv6_cidr_block",
        &[
            "2001:db8::/44",
            "2001:db8::/48",
            "2001:db8::/52",
            "2001:db8::/56",
            "2001:db8::/60",
            "2001:db8::/64",
            "2001:0DB8:0000:0000:0000:0000:0000:0001/64",
        ],
        &[
            "2001:db8::/40",
            "2001:db8::/45",
            "2001:db8::/63",
            "2001:db8::/65",
            "2001:db8:::1/64",
            "10.0.0.0/24",
        ],
    );
}

#[test]
fn ec2_vpc_and_subnet_ipv4_netmasks_are_anonymous_16_through_28() {
    for resource in ["Vpc", "Subnet"] {
        let native = schema(resource);
        let wire = resource_schema_to_protocol(&native).unwrap();
        let decoded = protocol_to_resource_schema(&wire).unwrap();
        for schema in [native, decoded] {
            let ty = field(&schema, "ipv4_netmask_length");
            assert!(matches!(
                ty.as_type().raw_shape(),
                RawShape::Int {
                    identity_layer: carina_core::schema::IntIdentityParts { identity: None, .. },
                    member_layer: carina_core::schema::IntMemberParts {
                        range: Some((Some(16), Some(28)))
                    }
                }
            ));
            for value in [0, 15, 16, 24, 28, 29, 32] {
                assert_eq!(ty.validate(&int(value)).is_ok(), (16..=28).contains(&value));
            }
        }
    }
}

#[test]
fn ec2_subnet_ipv6_netmask_is_the_documented_discrete_set() {
    let schema = schema("Subnet");
    let ty = field(&schema, "ipv6_netmask_length");
    for value in 0..=129 {
        assert_eq!(
            ty.validate(&int(value)).is_ok(),
            [44, 48, 52, 56, 60, 64].contains(&value),
            "{value}"
        );
    }
    assert!(ty.validate(&string("64")).is_err());
}

#[test]
fn ec2_subnet_local_network_interface_index_starts_at_one() {
    let schema = schema("Subnet");
    let ty = field(&schema, "enable_lni_at_device_index");
    // The Smithy carrier is Integer (i32), not an instance-dependent limit.
    for value in [
        i64::MIN,
        -1,
        0,
        1,
        2,
        i64::from(i32::MAX),
        i64::from(i32::MAX) + 1,
    ] {
        assert_eq!(
            ty.validate(&int(value)).is_ok(),
            (1..=i64::from(i32::MAX)).contains(&value),
            "{value}"
        );
    }
}

#[test]
fn ec2_amazon_side_asns_accept_both_private_ranges_and_reject_the_gap() {
    // carina-test-004, 2026-11-05: CreateVpnGateway rejects 64511 and
    // 65535, and accepts 4200000000. The transit gateway domain is documented
    // to use the same ranges; that resource was not part of the real probe.
    for (resource, path) in [
        ("TransitGateway", "options.amazon_side_asn"),
        ("VpnGateway", "amazon_side_asn"),
    ] {
        let schema = schema(resource);
        let ty = field(&schema, path);
        for value in [
            -1,
            0,
            64511,
            64512,
            65534,
            65535,
            4199999999,
            4200000000,
            4294967294,
            4294967295,
            i64::MAX,
        ] {
            assert_eq!(
                ty.validate(&int(value)).is_ok(),
                (64512..=65534).contains(&value) || (4200000000..=4294967294).contains(&value),
                "{resource}.{path} = {value}"
            );
        }
        assert!(ty.validate(&string("64512")).is_err());
    }
}

#[test]
fn ec2_vpn_gateway_asn_accepts_every_documented_legacy_value() {
    // https://aws.amazon.com/vpn/faqs/: Dublin 9059, Singapore 17493,
    // Tokyo 10124, all other regions 7224. The scalar has no region context.
    // carina-test-004: Tokyo accepts 10124, rejects another region's 7224,
    // and defaults to 64512 when the parameter is omitted.
    let vgw = schema("VpnGateway");
    let vgw_asn = field(&vgw, "amazon_side_asn");
    let rejected = [
        7224, 9059, 10124, 17493, 64512, 65534, 4200000000, 4294967294,
    ]
    .into_iter()
    .filter(|value| vgw_asn.validate(&int(*value)).is_err())
    .collect::<Vec<_>>();
    assert!(
        rejected.is_empty(),
        "VGW rejects documented ASNs: {rejected:?}"
    );
    for value in [
        7223, 7225, 9058, 9060, 10123, 10125, 17492, 17494, 64511, 65535, 4199999999, 4294967295,
    ] {
        assert!(
            vgw_asn.validate(&int(value)).is_err(),
            "VGW accepts undocumented ASN {value}"
        );
    }
    let tgw = schema("TransitGateway");
    let tgw_asn = field(&tgw, "options.amazon_side_asn");
    for value in [7224, 9059, 10124, 17493] {
        assert!(
            tgw_asn.validate(&int(value)).is_err(),
            "TGW accepts legacy VGW ASN {value}"
        );
    }
}

#[test]
fn ec2_gateway_asn_references_only_widen_from_private_to_legacy_capable() {
    use std::sync::Arc;
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        carina_core::schema::SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let registry = snapshot.schema_registry();
    let tgw = registry
        .get(
            "aws",
            "ec2.TransitGateway",
            carina_core::schema::SchemaKind::Resource,
        )
        .unwrap();
    let vgw = registry
        .get(
            "aws",
            "ec2.VpnGateway",
            carina_core::schema::SchemaKind::Resource,
        )
        .unwrap();
    let tgw_asn = field(&tgw, "options.amazon_side_asn");
    let vgw_asn = field(&vgw, "amazon_side_asn");
    let catalog = &snapshot.provider_context().custom_types;
    assert!(
        tgw.as_schema()
            .type_in_schema(tgw_asn.as_type())
            .is_assignable_to(vgw.assignment_sink(vgw_asn.as_type(), catalog))
    );
    assert!(
        !vgw.as_schema()
            .type_in_schema(vgw_asn.as_type())
            .is_assignable_to(tgw.assignment_sink(tgw_asn.as_type(), catalog)),
        "a legacy-capable VGW output cannot prove TGW's private-only domain"
    );
}

#[test]
fn ec2_transit_gateway_cidr_accepts_both_families_and_larger_networks() {
    // API_TransitGatewayRequestOptions; create-tgw examples clarify that
    // larger means a shorter prefix. The guide also excludes 169.254/16.
    strings(
        "TransitGateway",
        "options.transit_gateway_cidr_blocks[]",
        &[
            "10.0.0.0/24",
            "10.0.0.0/23",
            "10.0.0.1/16",
            "2001:db8::/64",
            "2001:db8::/63",
            "2001:0DB8:0000:0000:0000:0000:0000:0001/48",
        ],
        &[
            "10.0.0.0/25",
            "10.0.0.0/32",
            "2001:db8::/65",
            "2001:db8::/128",
            "169.254.0.0/24",
            "256.1.1.1/16",
            "2001:db8:::1/64",
            "not-cidr",
        ],
    );
}

#[test]
fn ec2_new_named_validators_survive_protocol_21_without_changing_carriers() {
    for (resource, path, identity) in [
        ("Eip", "address", "Ipv4Address"),
        (
            "FlowLog",
            "max_aggregation_interval",
            "aws.ec2.FlowLog.MaxAggregationInterval",
        ),
        ("SecurityGroup", "group_name", "aws.ec2.SecurityGroup.Name"),
        ("Subnet", "cidr_block", "aws.ec2.Subnet.Ipv4Cidr"),
        ("Subnet", "ipv6_cidr_block", "aws.ec2.Subnet.Ipv6Cidr"),
        (
            "Subnet",
            "ipv6_netmask_length",
            "aws.ec2.Subnet.Ipv6NetmaskLength",
        ),
        (
            "TransitGateway",
            "options.amazon_side_asn",
            "aws.ec2.AmazonSideAsn",
        ),
        (
            "TransitGateway",
            "options.transit_gateway_cidr_blocks[]",
            "aws.ec2.TransitGateway.CidrBlock",
        ),
        ("Vpc", "cidr_block", "aws.ec2.Vpc.Ipv4Cidr"),
        ("VpnGateway", "amazon_side_asn", "aws.AmazonSideAsn"),
    ] {
        let native = schema(resource);
        let wire = resource_schema_to_protocol(&native).unwrap();
        let decoded = protocol_to_resource_schema(&wire).unwrap();
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::to_value(resource_schema_to_protocol(&decoded).unwrap()).unwrap()
        );
        for schema in [native, decoded] {
            let ty = field(&schema, path);
            let actual = match ty.as_type().raw_shape() {
                RawShape::String {
                    identity_layer:
                        carina_core::schema::StringIdentityParts {
                            identity: Some(identity),
                            ..
                        },
                    ..
                }
                | RawShape::Int {
                    identity_layer:
                        carina_core::schema::IntIdentityParts {
                            identity: Some(identity),
                            ..
                        },
                    ..
                } => identity,
                other => panic!("{resource}.{path}: wrong carrier {other:?}"),
            };
            assert_eq!(actual.to_string(), identity);
        }
    }
}

#[test]
fn ec2_infra_network_shapes_and_existing_id_references_remain_valid() {
    // infra/usecases/registry/network/vpc.crn: one VPC and two public subnets.
    // infra/usecases/registry/publish/alb.crn: two security groups and their rules.
    // Those files use AWSCC. This AWS regression preserves their actual CIDRs,
    // zones, flags, descriptions, tags, and rule values. AWS requires explicit
    // group_name and description; AWSCC's inline rules are separate AWS resources.
    let source = r#"
        let vpc = aws.ec2.Vpc {
            cidr_block = '10.0.0.0/16'
            enable_dns_support = true
            enable_dns_hostnames = true
            tags = { Name = 'carina-registry-network-vpc' }
        }
        let public_subnet = aws.ec2.Subnet {
            vpc_id = vpc.vpc_id
            cidr_block = '10.0.0.0/24'
            availability_zone = 'ap-northeast-1a'
            map_public_ip_on_launch = true
            tags = { Name = 'carina-registry-network-public-a' }
        }
        let public_subnet_c = aws.ec2.Subnet {
            vpc_id = vpc.vpc_id
            cidr_block = '10.0.1.0/24'
            availability_zone = 'ap-northeast-1c'
            map_public_ip_on_launch = true
            tags = { Name = 'carina-registry-network-public-c' }
        }
        let service_sg = aws.ec2.SecurityGroup {
            vpc_id = vpc.vpc_id
            group_name = 'carina-registry-publish-api-service-sg'
            description = 'Publish API Fargate service security group'
            tags = { Name = 'carina-registry-publish-api-service-sg' }
        }
        let alb_sg = aws.ec2.SecurityGroup {
            vpc_id = vpc.vpc_id
            group_name = 'carina-registry-publish-api-alb-sg'
            description = 'Publish API ALB security group'
            tags = { Name = 'carina-registry-publish-api-alb-sg' }
        }
        aws.ec2.SecurityGroupEgress { group_id = service_sg.group_id ip_protocol = all cidr_ip = '0.0.0.0/0' }
        aws.ec2.SecurityGroupIngress { group_id = alb_sg.group_id ip_protocol = tcp from_port = 443 to_port = 443 cidr_ip = '0.0.0.0/0' description = 'HTTPS from anywhere' }
        aws.ec2.SecurityGroupEgress { group_id = alb_sg.group_id ip_protocol = tcp from_port = 8080 to_port = 8080 destination_security_group_id = service_sg.group_id description = 'Forwarded requests + health checks to the service' }
        aws.ec2.SecurityGroupIngress { group_id = service_sg.group_id ip_protocol = tcp from_port = 8080 to_port = 8080 source_security_group_id = alb_sg.group_id description = 'Publish API traffic from the ALB only' }
    "#;
    use std::{collections::HashSet, path::Path, sync::Arc};
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        carina_core::schema::SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    let parsed = carina_core::parser::parse_file(
        source,
        Path::new("infra-ec2-shapes.crn"),
        snapshot.provider_context(),
    )
    .unwrap();
    carina_core::validation::validate_resources(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::from(["aws".to_string()]),
        snapshot.provider_context(),
        &mut Vec::new(),
    )
    .unwrap();
    let bindings = carina_core::binding_index::BindingIndex::from_parsed(
        &parsed,
        snapshot.schema_registry(),
        snapshot.provider_context(),
    );
    let configuration = carina_core::config_loader::load_buffer_configuration(
        &parsed,
        snapshot.schema_registry(),
        snapshot.provider_context(),
    );
    carina_core::validation::validate_resource_ref_types(
        configuration.judged(),
        &HashSet::new(),
        &bindings,
        &mut Vec::new(),
    )
    .unwrap();
}

#[test]
fn ec2_identified_inputs_require_matching_reference_identities() {
    use std::{collections::HashSet, path::Path, sync::Arc};
    let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(carina_provider_aws::AwsProviderFactory)]),
        carina_core::schema::SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    );
    for (declaration, succeeds) in [
        ("aws.ec2.Eip { address = source.description }", false),
        ("aws.ec2.Vpc { cidr_block = source.description }", false),
        (
            "aws.ec2.SecurityGroup { description = 'sink' group_name = source.description }",
            false,
        ),
        (
            "let vpc = aws.ec2.Vpc { cidr_block = '10.0.0.0/16' } aws.ec2.Vpc { cidr_block = vpc.cidr_block }",
            true,
        ),
        (
            "aws.ec2.FlowLog { resource_type = vpc resource_ids = ['vpc-12345678'] traffic_type = all log_group_name = source.description }",
            false,
        ),
        (
            "let group = aws.logs.LogGroup { log_group_name = '/aws/vpc/flow-logs' } aws.ec2.FlowLog { resource_type = vpc resource_ids = ['vpc-12345678'] traffic_type = all log_group_name = group.log_group_name }",
            true,
        ),
    ] {
        let source = format!(
            "let source = aws.ec2.SecurityGroup {{ group_name = 'source' description = 'compatible-value' }}\n{declaration}"
        );
        let parsed = carina_core::parser::parse_file(
            &source,
            Path::new("ec2-references.crn"),
            snapshot.provider_context(),
        )
        .unwrap();
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &HashSet::from(["aws".to_string()]),
            snapshot.provider_context(),
            &mut Vec::new(),
        )
        .unwrap();
        let bindings = carina_core::binding_index::BindingIndex::from_parsed(
            &parsed,
            snapshot.schema_registry(),
            snapshot.provider_context(),
        );
        let configuration = carina_core::config_loader::load_buffer_configuration(
            &parsed,
            snapshot.schema_registry(),
            snapshot.provider_context(),
        );
        let result = carina_core::validation::validate_resource_ref_types(
            configuration.judged(),
            &HashSet::new(),
            &bindings,
            &mut Vec::new(),
        );
        assert_eq!(result.is_ok(), succeeds, "{declaration}: {result:?}");
    }
}
