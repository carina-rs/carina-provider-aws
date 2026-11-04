//! Scripted EC2 Query responses exercise the actual SDK and read seam.
//! They are compatibility witnesses, not evidence of real AWS behavior.
#![cfg(not(target_arch = "wasm32"))]

use std::future::{Future, ready};
use std::pin::Pin;

use carina_core::provider::{AppliedIdentifier, Provider, ReadRequest};
use carina_core::resource::{ConcreteValue, EnumValueResolver, ResourceId, Value};
use carina_provider_aws::{AwsProvider, BoundedSdkConfig};
use indexmap::IndexMap;
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

struct Ec2ReadService {
    action: &'static str,
    payload: String,
}

impl MockService for Ec2ReadService {
    fn service_name(&self) -> &str {
        "ec2"
    }
    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://ec2\.[^/]+\.amazonaws\.com"]
    }
    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let body = std::str::from_utf8(&request.body).unwrap();
        let action = body
            .split('&')
            .find_map(|part| part.strip_prefix("Action="))
            .unwrap();
        let payload = if action == "DescribeVpcAttribute" {
            "<vpcId>vpc-12345678</vpcId><enableDnsSupport><value>true</value></enableDnsSupport><enableDnsHostnames><value>true</value></enableDnsHostnames>"
        } else {
            assert_eq!(action, self.action);
            &self.payload
        };
        Box::pin(ready(MockResponse::xml(
            200,
            format!(
                "<{action}Response xmlns=\"http://ec2.amazonaws.com/doc/2016-11-15/\">{payload}</{action}Response>"
            ),
        )))
    }
}

async fn read(
    resource: &str,
    id: &str,
    action: &'static str,
    payload: String,
) -> carina_core::resource::SnapshotState {
    let mock = MockAws::builder()
        .with_service(Ec2ReadService { action, payload })
        .build();
    let config = BoundedSdkConfig::for_test(mock.sdk_config("us-east-1").await);
    let provider = AwsProvider::from_sdk_config("us-east-1".into(), vec![], vec![], &config);
    provider
        .read(
            &ResourceId::with_provider_identity("aws", format!("ec2.{resource}"), "read", None),
            &AppliedIdentifier::new(id).unwrap(),
            ReadRequest,
        )
        .await
        .unwrap_or_else(|error| panic!("{resource}: {error}"))
        .into_state()
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}
fn int(value: i64) -> Value {
    Value::Concrete(ConcreteValue::Int(value))
}

#[tokio::test]
async fn ec2_strict_reads_preserve_eip_and_security_group_spellings() {
    let state = read("Eip", "eipalloc-12345678", "DescribeAddresses",
        "<addressesSet><item><allocationId>eipalloc-12345678</allocationId><publicIp>198.51.100.10</publicIp><domain>vpc</domain></item></addressesSet>".into()).await;
    assert_eq!(
        state.attributes.get("address"),
        Some(&string("198.51.100.10").to_snapshot())
    );
    // carina-test-004, 2026-11-05: these two probe names read back verbatim.
    // Escape the Query XML response without changing the returned name.
    for name in [
        "default",
        "Carina Mixed Case",
        "service/name (production)",
        "SG-Probe",
        "probe ok._-:/()#,@[]+=&;{}!$*",
    ] {
        let encoded_name = name.replace('&', "&amp;");
        let state = read("SecurityGroup", "sg-12345678", "DescribeSecurityGroups",
            format!("<securityGroupInfo><item><groupId>sg-12345678</groupId><groupName>{encoded_name}</groupName><groupDescription>infra description</groupDescription><vpcId>vpc-12345678</vpcId></item></securityGroupInfo>")).await;
        assert_eq!(
            state.attributes.get("group_name"),
            Some(&string(name).to_snapshot())
        );
    }
}

#[tokio::test]
async fn ec2_new_subnet_read_omits_absent_lni_device_index() {
    // carina-test-004, ap-northeast-1, 2026-11-05: a newly created subnet
    // omits EnableLniAtDeviceIndex. Absence must remain absence, not eth0.
    let state = read("Subnet", "subnet-12345678", "DescribeSubnets",
        "<subnetSet><item><subnetId>subnet-12345678</subnetId><vpcId>vpc-12345678</vpcId><cidrBlock>10.0.0.0/28</cidrBlock></item></subnetSet>".into()).await;
    assert!(!state.attributes.contains_key("enable_lni_at_device_index"));
    assert_eq!(
        state.attributes.get("cidr_block"),
        Some(&string("10.0.0.0/28").to_snapshot())
    );
}

#[tokio::test]
async fn ec2_strict_reads_preserve_vpc_and_subnet_cidr_contracts() {
    let state = read("Vpc", "vpc-12345678", "DescribeVpcs",
        "<vpcSet><item><vpcId>vpc-12345678</vpcId><cidrBlock>100.68.0.0/18</cidrBlock><instanceTenancy>default</instanceTenancy></item></vpcSet>".into()).await;
    assert_eq!(
        state.attributes.get("cidr_block"),
        Some(&string("100.68.0.0/18").to_snapshot())
    );
    assert_eq!(
        state.attributes.get("ipv4_netmask_length"),
        Some(&int(18).to_snapshot())
    );
    for prefix in [44, 48, 52, 56, 60, 64] {
        let state = read("Subnet", "subnet-12345678", "DescribeSubnets",
            format!("<subnetSet><item><subnetId>subnet-12345678</subnetId><vpcId>vpc-12345678</vpcId><cidrBlock>10.0.0.0/24</cidrBlock><enableLniAtDeviceIndex>1</enableLniAtDeviceIndex><ipv6CidrBlockAssociationSet><item><ipv6CidrBlock>2001:0DB8:0000:0000:0000:0000:0000:0000/{prefix}</ipv6CidrBlock><ipv6CidrBlockState><state>associated</state></ipv6CidrBlockState></item></ipv6CidrBlockAssociationSet></item></subnetSet>")).await;
        assert_eq!(
            state.attributes.get("ipv6_cidr_block"),
            Some(&string(&format!("2001:db8::/{prefix}")).to_snapshot())
        );
        assert_eq!(
            state.attributes.get("ipv6_netmask_length"),
            Some(&int(prefix).to_snapshot())
        );
    }
}

#[tokio::test]
async fn ec2_strict_reads_accept_both_private_asn_ranges_and_transit_cidr_families() {
    // carina-test-004, 2026-11-05: VPN CreateVpnGateway accepts
    // 4200000000 and DescribeVpnGateways returns that same numeric value.
    for asn in [64512, 65534, 4200000000_i64, 4294967294] {
        let state = read("TransitGateway", "tgw-12345678", "DescribeTransitGateways",
            format!("<transitGatewaySet><item><transitGatewayId>tgw-12345678</transitGatewayId><state>available</state><options><amazonSideAsn>{asn}</amazonSideAsn><transitGatewayCidrBlocks><item>10.0.0.0/24</item><item>2001:0DB8:0000:0000:0000:0000:0000:0000/64</item></transitGatewayCidrBlocks></options></item></transitGatewaySet>")).await;
        let Some(Value::Concrete(ConcreteValue::Map(options))) = state.attributes.get("options")
        else {
            panic!("missing options")
        };
        assert_eq!(
            options.get("amazon_side_asn"),
            Some(&int(asn).to_snapshot())
        );
        let expected = Value::Concrete(ConcreteValue::List(vec![
            string("10.0.0.0/24"),
            string("2001:0DB8:0000:0000:0000:0000:0000:0000/64"),
        ]));
        assert_eq!(
            options.get("transit_gateway_cidr_blocks"),
            Some(&expected.to_snapshot())
        );
        let state = read("VpnGateway", "vgw-12345678", "DescribeVpnGateways",
            format!("<vpnGatewaySet><item><vpnGatewayId>vgw-12345678</vpnGatewayId><state>available</state><type>ipsec.1</type><amazonSideAsn>{asn}</amazonSideAsn></item></vpnGatewaySet>")).await;
        assert_eq!(
            state.attributes.get("amazon_side_asn"),
            Some(&int(asn).to_snapshot())
        );
    }
}

#[tokio::test]
async fn ec2_vpn_gateway_reads_preserve_legacy_asns_and_default() {
    let schema = carina_provider_aws::schemas::all_schemas()
        .into_iter()
        .find(|schema| schema.as_schema().resource_type() == "ec2.VpnGateway")
        .unwrap();
    let asn_type = schema.effective_attribute("amazon_side_asn").unwrap();
    for asn in [7224, 9059, 10124, 17493, 64512] {
        let state = read("VpnGateway", "vgw-12345678", "DescribeVpnGateways",
            format!("<vpnGatewaySet><item><vpnGatewayId>vgw-12345678</vpnGatewayId><state>available</state><type>ipsec.1</type><amazonSideAsn>{asn}</amazonSideAsn></item></vpnGatewaySet>")).await;
        assert_eq!(
            state.attributes.get("amazon_side_asn"),
            Some(&int(asn).to_snapshot())
        );
        assert!(
            asn_type.validate(&int(asn)).is_ok(),
            "VGW schema rejects read-back ASN {asn}"
        );
    }
}

#[tokio::test]
async fn ec2_strict_reads_preserve_flow_log_format_names_and_tag_keys() {
    let schema = carina_provider_aws::schemas::generated::ec2::flow_log::ec2_flow_log_config()
        .schema
        .validate()
        .expect("FlowLog schema");
    let tag_fields_attribute = schema
        .effective_attribute("tag_field_specifications")
        .expect("FlowLog tag field specifications attribute");
    let resource_type = schema
        .effective_node_at_path(&["tag_field_specifications".into(), "resource_type".into()])
        .expect("FlowLog tag resource type");
    let network_interface = EnumValueResolver::new(resource_type.field_type())
        .resolve_state_text("network-interface")
        .expect("network-interface read spelling");
    let expected_tag_fields: Value = Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
        ConcreteValue::Map(IndexMap::from([
            (
                "resource_type".into(),
                Value::Concrete(ConcreteValue::CanonicalEnum(network_interface)),
            ),
            (
                "tag_keys".into(),
                Value::Concrete(ConcreteValue::List(vec![
                    string("Name"),
                    string("Environment"),
                ])),
            ),
        ])),
    )]));
    for interval in [60, 600] {
        let state = read("FlowLog", "fl-12345678", "DescribeFlowLogs",
            format!("<flowLogSet><item><flowLogId>fl-12345678</flowLogId><resourceId>vpc-12345678</resourceId><flowLogStatus>ACTIVE</flowLogStatus><logGroupName>aws/existing-flow-logs</logGroupName><logFormat>${{version}} ${{interface-tag-2}}</logFormat><maxAggregationInterval>{interval}</maxAggregationInterval><tagFieldSpecificationSet><item><resourceType>network-interface</resourceType><tagKeySet><item>Name</item><item>Environment</item></tagKeySet></item></tagFieldSpecificationSet></item></flowLogSet>")).await;
        assert_eq!(
            state.attributes.get("log_format"),
            Some(&string("${version} ${interface-tag-2}").to_snapshot())
        );
        assert_eq!(
            state.attributes.get("log_group_name"),
            Some(&string("aws/existing-flow-logs").to_snapshot())
        );
        assert_eq!(
            state.attributes.get("max_aggregation_interval"),
            Some(&int(interval).to_snapshot())
        );
        let tag_fields = state
            .attributes
            .get("tag_field_specifications")
            .expect("FlowLog read must preserve tag_field_specifications");
        assert_eq!(tag_fields, &expected_tag_fields.to_snapshot());
        let tag_fields_value = tag_fields
            .to_value_at("tag_field_specifications")
            .expect("scripted read contains available tag fields");
        tag_fields_attribute
            .validate(&tag_fields_value)
            .expect("FlowLog effective attribute must validate the read tag fields");
    }
}
