//! Tests for generated provider methods and integration patterns

use std::collections::HashMap;

use indexmap::IndexMap;

use carina_core::provider::ProviderNormalizer;
use carina_core::resource::{ConcreteValue, Value};

use crate::{AwsNormalizer, AwsProvider};

// --- extract_ec2_vpc_attributes tests ---

#[test]
fn test_extract_ec2_vpc_attributes() {
    let vpc = aws_sdk_ec2::types::Vpc::builder()
        .vpc_id("vpc-12345678")
        .cidr_block("10.0.0.0/16")
        .instance_tenancy(aws_sdk_ec2::types::Tenancy::Default)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_vpc_attributes(&vpc, &mut attributes);
    assert_eq!(identifier, Some("vpc-12345678".to_string()));
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("cidr_block"),
        Some(&Value::Concrete(ConcreteValue::String(
            "10.0.0.0/16".to_string()
        )))
    );
    assert_eq!(
        attributes.get("instance_tenancy"),
        Some(&Value::Concrete(ConcreteValue::String(
            "default".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_vpc_attributes_minimal() {
    let vpc = aws_sdk_ec2::types::Vpc::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_vpc_attributes(&vpc, &mut attributes);
    assert_eq!(identifier, None);
    assert!(attributes.is_empty());
}

// --- extract_ec2_subnet_attributes tests ---

#[test]
fn test_extract_ec2_subnet_attributes() {
    let subnet = aws_sdk_ec2::types::Subnet::builder()
        .subnet_id("subnet-12345678")
        .vpc_id("vpc-12345678")
        .cidr_block("10.0.1.0/24")
        .availability_zone("ap-northeast-1a")
        .map_public_ip_on_launch(false)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_subnet_attributes(&subnet, &mut attributes);
    assert_eq!(identifier, Some("subnet-12345678".to_string()));
    assert_eq!(
        attributes.get("subnet_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "subnet-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("cidr_block"),
        Some(&Value::Concrete(ConcreteValue::String(
            "10.0.1.0/24".to_string()
        )))
    );
    assert_eq!(
        attributes.get("availability_zone"),
        Some(&Value::Concrete(ConcreteValue::String(
            "ap-northeast-1a".to_string()
        )))
    );
    assert_eq!(
        attributes.get("map_public_ip_on_launch"),
        Some(&Value::Concrete(ConcreteValue::Bool(false)))
    );
}

#[test]
fn test_extract_ec2_subnet_attributes_minimal() {
    let subnet = aws_sdk_ec2::types::Subnet::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_subnet_attributes(&subnet, &mut attributes);
    assert_eq!(identifier, None);
}

#[test]
fn test_extract_ec2_subnet_attributes_with_private_dns_name_options() {
    use aws_sdk_ec2::types::{HostnameType, PrivateDnsNameOptionsOnLaunch};

    let dns_options = PrivateDnsNameOptionsOnLaunch::builder()
        .hostname_type(HostnameType::IpName)
        .enable_resource_name_dns_a_record(true)
        .enable_resource_name_dns_aaaa_record(false)
        .build();

    let subnet = aws_sdk_ec2::types::Subnet::builder()
        .subnet_id("subnet-12345678")
        .vpc_id("vpc-12345678")
        .cidr_block("10.0.1.0/24")
        .private_dns_name_options_on_launch(dns_options)
        .build();

    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_subnet_attributes(&subnet, &mut attributes);
    assert_eq!(identifier, Some("subnet-12345678".to_string()));

    // Verify the struct is extracted as a Value::Map
    let dns_value = attributes
        .get("private_dns_name_options_on_launch")
        .expect("private_dns_name_options_on_launch should be present");

    if let Value::Concrete(ConcreteValue::Map(fields)) = dns_value {
        assert_eq!(
            fields.get("hostname_type"),
            Some(&Value::Concrete(ConcreteValue::String(
                "ip-name".to_string()
            )))
        );
        assert_eq!(
            fields.get("enable_resource_name_dns_a_record"),
            Some(&Value::Concrete(ConcreteValue::Bool(true)))
        );
        assert_eq!(
            fields.get("enable_resource_name_dns_aaaa_record"),
            Some(&Value::Concrete(ConcreteValue::Bool(false)))
        );
    } else {
        panic!(
            "Expected Value::Map for private_dns_name_options_on_launch, got {:?}",
            dns_value
        );
    }
}

// --- extract_ec2_internet_gateway_attributes tests ---

#[test]
fn test_extract_ec2_internet_gateway_attributes() {
    let igw = aws_sdk_ec2::types::InternetGateway::builder()
        .internet_gateway_id("igw-12345678")
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_internet_gateway_attributes(&igw, &mut attributes);
    assert_eq!(identifier, Some("igw-12345678".to_string()));
    assert_eq!(
        attributes.get("internet_gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "igw-12345678".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_internet_gateway_attributes_minimal() {
    let igw = aws_sdk_ec2::types::InternetGateway::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_internet_gateway_attributes(&igw, &mut attributes);
    assert_eq!(identifier, None);
    assert!(attributes.is_empty());
}

// --- extract_ec2_route_table_attributes tests ---

#[test]
fn test_extract_ec2_route_table_attributes() {
    let rt = aws_sdk_ec2::types::RouteTable::builder()
        .route_table_id("rtb-12345678")
        .vpc_id("vpc-12345678")
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_route_table_attributes(&rt, &mut attributes);
    assert_eq!(identifier, Some("rtb-12345678".to_string()));
    assert_eq!(
        attributes.get("route_table_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "rtb-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_route_table_attributes_minimal() {
    let rt = aws_sdk_ec2::types::RouteTable::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_route_table_attributes(&rt, &mut attributes);
    assert_eq!(identifier, None);
}

// --- extract_ec2_route_attributes tests ---

#[test]
fn test_extract_ec2_route_attributes() {
    let route = aws_sdk_ec2::types::Route::builder()
        .destination_cidr_block("0.0.0.0/0")
        .gateway_id("igw-12345678")
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_route_attributes(&route, &mut attributes);
    // Route extraction returns None (no single identifier)
    assert_eq!(identifier, None);
    assert_eq!(
        attributes.get("destination_cidr_block"),
        Some(&Value::Concrete(ConcreteValue::String(
            "0.0.0.0/0".to_string()
        )))
    );
    assert_eq!(
        attributes.get("gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "igw-12345678".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_route_attributes_with_nat_gateway() {
    let route = aws_sdk_ec2::types::Route::builder()
        .destination_cidr_block("10.0.0.0/8")
        .nat_gateway_id("nat-12345678")
        .build();
    let mut attributes = HashMap::new();
    AwsProvider::extract_ec2_route_attributes(&route, &mut attributes);
    assert_eq!(
        attributes.get("destination_cidr_block"),
        Some(&Value::Concrete(ConcreteValue::String(
            "10.0.0.0/8".to_string()
        )))
    );
    assert_eq!(
        attributes.get("nat_gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "nat-12345678".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_route_attributes_ignores_unsupported() {
    // transit_gateway_id is not in the schema, so it should not be extracted
    let route = aws_sdk_ec2::types::Route::builder()
        .destination_cidr_block("172.16.0.0/12")
        .transit_gateway_id("tgw-12345678")
        .build();
    let mut attributes = HashMap::new();
    AwsProvider::extract_ec2_route_attributes(&route, &mut attributes);
    assert_eq!(
        attributes.get("destination_cidr_block"),
        Some(&Value::Concrete(ConcreteValue::String(
            "172.16.0.0/12".to_string()
        )))
    );
    assert_eq!(attributes.get("transit_gateway_id"), None);
}

// --- extract_ec2_security_group_attributes tests ---

#[test]
fn test_extract_ec2_security_group_attributes() {
    let sg = aws_sdk_ec2::types::SecurityGroup::builder()
        .group_id("sg-12345678")
        .group_name("test-sg")
        .description("Test security group")
        .vpc_id("vpc-12345678")
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_security_group_attributes(&sg, &mut attributes);
    assert_eq!(identifier, Some("sg-12345678".to_string()));
    assert_eq!(
        attributes.get("group_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "sg-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("group_name"),
        Some(&Value::Concrete(ConcreteValue::String(
            "test-sg".to_string()
        )))
    );
    assert_eq!(
        attributes.get("description"),
        Some(&Value::Concrete(ConcreteValue::String(
            "Test security group".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_security_group_attributes_minimal() {
    let sg = aws_sdk_ec2::types::SecurityGroup::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_security_group_attributes(&sg, &mut attributes);
    assert_eq!(identifier, None);
}

// --- extract_ec2_security_group_ingress_attributes tests ---

#[test]
fn test_extract_ec2_security_group_ingress_attributes() {
    let rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
        .security_group_rule_id("sgr-12345678")
        .group_id("sg-12345678")
        .ip_protocol("tcp")
        .from_port(443)
        .to_port(443)
        .description("HTTPS")
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_security_group_ingress_attributes(&rule, &mut attributes);
    assert_eq!(identifier, Some("sgr-12345678".to_string()));
    assert_eq!(
        attributes.get("security_group_rule_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "sgr-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("group_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "sg-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("ip_protocol"),
        Some(&Value::Concrete(ConcreteValue::String("tcp".to_string())))
    );
    assert_eq!(
        attributes.get("from_port"),
        Some(&Value::Concrete(ConcreteValue::Int(443)))
    );
    assert_eq!(
        attributes.get("to_port"),
        Some(&Value::Concrete(ConcreteValue::Int(443)))
    );
    assert_eq!(
        attributes.get("description"),
        Some(&Value::Concrete(ConcreteValue::String("HTTPS".to_string())))
    );
}

#[test]
fn test_extract_ec2_security_group_ingress_attributes_with_prefix_list() {
    let rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
        .security_group_rule_id("sgr-99999999")
        .group_id("sg-12345678")
        .ip_protocol("tcp")
        .from_port(80)
        .to_port(80)
        .prefix_list_id("pl-12345678")
        .build();
    let mut attributes = HashMap::new();
    AwsProvider::extract_ec2_security_group_ingress_attributes(&rule, &mut attributes);
    assert_eq!(
        attributes.get("source_prefix_list_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "pl-12345678".to_string()
        )))
    );
}

// --- extract_ec2_security_group_egress_attributes tests ---

#[test]
fn test_extract_ec2_security_group_egress_attributes() {
    let rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
        .security_group_rule_id("sgr-87654321")
        .group_id("sg-12345678")
        .ip_protocol("-1")
        .from_port(0)
        .to_port(0)
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_security_group_egress_attributes(&rule, &mut attributes);
    assert_eq!(identifier, Some("sgr-87654321".to_string()));
    assert_eq!(
        attributes.get("group_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "sg-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("ip_protocol"),
        Some(&Value::Concrete(ConcreteValue::String("-1".to_string())))
    );
    assert_eq!(
        attributes.get("from_port"),
        Some(&Value::Concrete(ConcreteValue::Int(0)))
    );
    assert_eq!(
        attributes.get("to_port"),
        Some(&Value::Concrete(ConcreteValue::Int(0)))
    );
}

#[test]
fn test_extract_ec2_security_group_egress_attributes_with_prefix_list() {
    let rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
        .security_group_rule_id("sgr-11111111")
        .group_id("sg-12345678")
        .ip_protocol("tcp")
        .from_port(443)
        .to_port(443)
        .prefix_list_id("pl-87654321")
        .build();
    let mut attributes = HashMap::new();
    AwsProvider::extract_ec2_security_group_egress_attributes(&rule, &mut attributes);
    assert_eq!(
        attributes.get("destination_prefix_list_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "pl-87654321".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_security_group_egress_attributes_with_ipv6() {
    let rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
        .security_group_rule_id("sgr-22222222")
        .group_id("sg-12345678")
        .ip_protocol("-1")
        .from_port(0)
        .to_port(0)
        .cidr_ipv6("::/0")
        .build();
    let mut attributes = HashMap::new();
    AwsProvider::extract_ec2_security_group_egress_attributes(&rule, &mut attributes);
    assert_eq!(
        attributes.get("cidr_ipv6"),
        Some(&Value::Concrete(ConcreteValue::String("::/0".to_string())))
    );
}

// --- EC2 route table route extraction from describe response ---

#[test]
fn test_route_table_routes_extraction() {
    // Simulates the route extraction logic in read_ec2_route_table
    let route1 = aws_sdk_ec2::types::Route::builder()
        .destination_cidr_block("10.0.0.0/16")
        .gateway_id("local")
        .build();
    let route2 = aws_sdk_ec2::types::Route::builder()
        .destination_cidr_block("0.0.0.0/0")
        .gateway_id("igw-12345678")
        .build();

    let rt = aws_sdk_ec2::types::RouteTable::builder()
        .route_table_id("rtb-12345678")
        .vpc_id("vpc-12345678")
        .routes(route1)
        .routes(route2)
        .build();

    // Replicate route extraction logic from read_ec2_route_table
    let mut routes_list = Vec::new();
    for route in rt.routes() {
        let mut route_map = IndexMap::new();
        if let Some(dest) = route.destination_cidr_block() {
            route_map.insert(
                "destination".to_string(),
                Value::Concrete(ConcreteValue::String(dest.to_string())),
            );
        }
        if let Some(gw) = route.gateway_id() {
            route_map.insert(
                "gateway_id".to_string(),
                Value::Concrete(ConcreteValue::String(gw.to_string())),
            );
        }
        if !route_map.is_empty() {
            routes_list.push(Value::Concrete(ConcreteValue::Map(route_map)));
        }
    }

    assert_eq!(routes_list.len(), 2);
    if let Value::Concrete(ConcreteValue::Map(ref map)) = routes_list[0] {
        assert_eq!(
            map.get("destination"),
            Some(&Value::Concrete(ConcreteValue::String(
                "10.0.0.0/16".to_string()
            )))
        );
        assert_eq!(
            map.get("gateway_id"),
            Some(&Value::Concrete(ConcreteValue::String("local".to_string())))
        );
    }
    if let Value::Concrete(ConcreteValue::Map(ref map)) = routes_list[1] {
        assert_eq!(
            map.get("destination"),
            Some(&Value::Concrete(ConcreteValue::String(
                "0.0.0.0/0".to_string()
            )))
        );
        assert_eq!(
            map.get("gateway_id"),
            Some(&Value::Concrete(ConcreteValue::String(
                "igw-12345678".to_string()
            )))
        );
    }
}

#[test]
fn test_route_table_routes_extraction_empty() {
    let rt = aws_sdk_ec2::types::RouteTable::builder()
        .route_table_id("rtb-12345678")
        .build();
    assert!(rt.routes().is_empty());
}

// --- Internet Gateway attachment extraction ---

#[test]
fn test_internet_gateway_attachment_extraction() {
    // Simulates the vpc_id extraction from IGW attachments
    let attachment = aws_sdk_ec2::types::InternetGatewayAttachment::builder()
        .vpc_id("vpc-12345678")
        .state(aws_sdk_ec2::types::AttachmentStatus::from("available"))
        .build();
    let igw = aws_sdk_ec2::types::InternetGateway::builder()
        .internet_gateway_id("igw-12345678")
        .attachments(attachment)
        .build();

    // Replicate logic from read_ec2_internet_gateway
    let mut attributes = HashMap::new();
    if let Some(att) = igw.attachments().first()
        && let Some(vpc_id) = att.vpc_id()
    {
        attributes.insert(
            "vpc_id".to_string(),
            Value::Concrete(ConcreteValue::String(vpc_id.to_string())),
        );
    }

    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
}

#[test]
fn test_internet_gateway_no_attachment() {
    let igw = aws_sdk_ec2::types::InternetGateway::builder()
        .internet_gateway_id("igw-12345678")
        .build();

    let mut attributes = HashMap::new();
    if let Some(att) = igw.attachments().first()
        && let Some(vpc_id) = att.vpc_id()
    {
        attributes.insert(
            "vpc_id".to_string(),
            Value::Concrete(ConcreteValue::String(vpc_id.to_string())),
        );
    }

    assert!(!attributes.contains_key("vpc_id"));
}

// --- extract_ec2_subnet_attributes with map_public_ip_on_launch true ---

#[test]
fn test_extract_ec2_subnet_attributes_map_public_ip_true() {
    let subnet = aws_sdk_ec2::types::Subnet::builder()
        .subnet_id("subnet-12345678")
        .vpc_id("vpc-12345678")
        .cidr_block("10.0.1.0/24")
        .availability_zone("ap-northeast-1a")
        .map_public_ip_on_launch(true)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_subnet_attributes(&subnet, &mut attributes);
    assert_eq!(identifier, Some("subnet-12345678".to_string()));
    assert_eq!(
        attributes.get("map_public_ip_on_launch"),
        Some(&Value::Concrete(ConcreteValue::Bool(true)))
    );
}

#[tokio::test]
async fn test_subnet_availability_zone_survives_normalize_desired() {
    let mut resource =
        carina_core::resource::Resource::with_provider("aws", "ec2.Subnet", "test-subnet", None);
    resource.set_attr(
        "availability_zone".to_string(),
        Value::Concrete(ConcreteValue::String("ap-northeast-1a".to_string())),
    );
    let mut resources = vec![resource];

    AwsNormalizer.normalize_desired(&mut resources).await;

    assert_eq!(
        resources[0].get_attr("availability_zone"),
        Some(&Value::Concrete(ConcreteValue::String(
            "ap-northeast-1a".to_string()
        )))
    );
}

// --- Subnet DNS hostname_type enum conversion ---

#[test]
fn test_subnet_hostname_type_canonical_value_to_aws_sdk() {
    use aws_sdk_ec2::types::HostnameType;

    // Core sends the provider the canonical API spelling.
    let aws_value = "ip-name";
    let hostname_type = HostnameType::from(aws_value);
    assert_eq!(hostname_type, HostnameType::IpName);

    let aws_value2 = "resource-name";
    let hostname_type2 = HostnameType::from(aws_value2);
    assert_eq!(hostname_type2, HostnameType::ResourceName);
}

// --- Subnet modify_subnet_attributes: DNS options must be separate API calls ---
// The AWS ModifySubnetAttribute API only allows modifying one attribute at a time.
// See: https://docs.aws.amazon.Com/AWSEC2/latest/APIReference/API_ModifySubnetAttribute.html
// "You can only modify one attribute at a time."
// This test verifies that private_dns_name_options_on_launch fields are parsed
// correctly for separate API calls.

#[test]
fn test_subnet_dns_options_fields_parsed_separately() {
    // Simulate the attributes map that would be passed to modify_subnet_attributes
    let mut fields = HashMap::new();
    fields.insert(
        "hostname_type".to_string(),
        Value::Concrete(ConcreteValue::String("ip-name".to_string())),
    );
    fields.insert(
        "enable_resource_name_dns_a_record".to_string(),
        Value::Concrete(ConcreteValue::Bool(true)),
    );
    fields.insert(
        "enable_resource_name_dns_aaaa_record".to_string(),
        Value::Concrete(ConcreteValue::Bool(false)),
    );

    // Each field should be independently extractable for separate API calls
    if let Some(Value::Concrete(ConcreteValue::String(ht))) = fields.get("hostname_type") {
        assert_eq!(
            aws_sdk_ec2::types::HostnameType::from(ht.as_str()),
            aws_sdk_ec2::types::HostnameType::IpName
        );
    } else {
        panic!("hostname_type should be present and a String");
    }

    if let Some(Value::Concrete(ConcreteValue::Bool(v))) =
        fields.get("enable_resource_name_dns_a_record")
    {
        assert!(*v);
    } else {
        panic!("enable_resource_name_dns_a_record should be present and a Bool");
    }

    if let Some(Value::Concrete(ConcreteValue::Bool(v))) =
        fields.get("enable_resource_name_dns_aaaa_record")
    {
        assert!(!(*v));
    } else {
        panic!("enable_resource_name_dns_aaaa_record should be present and a Bool");
    }
}

// --- extract_ec2_eip_attributes tests ---

#[test]
fn test_extract_ec2_eip_attributes() {
    let addr = aws_sdk_ec2::types::Address::builder()
        .allocation_id("eipalloc-12345678")
        .domain(aws_sdk_ec2::types::DomainType::Vpc)
        .public_ip("203.0.113.1")
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_eip_attributes(&addr, &mut attributes);
    assert_eq!(identifier, Some("eipalloc-12345678".to_string()));
    assert_eq!(
        attributes.get("allocation_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "eipalloc-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("domain"),
        Some(&Value::Concrete(ConcreteValue::String("vpc".to_string())))
    );
    assert_eq!(
        attributes.get("public_ip"),
        Some(&Value::Concrete(ConcreteValue::String(
            "203.0.113.1".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_eip_attributes_minimal() {
    let addr = aws_sdk_ec2::types::Address::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_eip_attributes(&addr, &mut attributes);
    assert_eq!(identifier, None);
    assert!(attributes.is_empty());
}

// --- extract_ec2_nat_gateway_attributes tests ---

#[test]
fn test_extract_ec2_nat_gateway_attributes() {
    let nat_addr = aws_sdk_ec2::types::NatGatewayAddress::builder()
        .allocation_id("eipalloc-12345678")
        .build();
    let ngw = aws_sdk_ec2::types::NatGateway::builder()
        .nat_gateway_id("nat-12345678")
        .subnet_id("subnet-12345678")
        .connectivity_type(aws_sdk_ec2::types::ConnectivityType::Public)
        .nat_gateway_addresses(nat_addr)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_nat_gateway_attributes(&ngw, &mut attributes);
    assert_eq!(identifier, Some("nat-12345678".to_string()));
    assert_eq!(
        attributes.get("nat_gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "nat-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("subnet_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "subnet-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("connectivity_type"),
        Some(&Value::Concrete(ConcreteValue::String(
            "public".to_string()
        )))
    );
    assert_eq!(
        attributes.get("allocation_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "eipalloc-12345678".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_nat_gateway_attributes_minimal() {
    let ngw = aws_sdk_ec2::types::NatGateway::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_nat_gateway_attributes(&ngw, &mut attributes);
    assert_eq!(identifier, None);
}

#[test]
fn test_extract_ec2_nat_gateway_attributes_private() {
    let ngw = aws_sdk_ec2::types::NatGateway::builder()
        .nat_gateway_id("nat-87654321")
        .subnet_id("subnet-87654321")
        .connectivity_type(aws_sdk_ec2::types::ConnectivityType::Private)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_nat_gateway_attributes(&ngw, &mut attributes);
    assert_eq!(identifier, Some("nat-87654321".to_string()));
    assert_eq!(
        attributes.get("connectivity_type"),
        Some(&Value::Concrete(ConcreteValue::String(
            "private".to_string()
        )))
    );
    // Private NAT gateways don't have allocation_id
    assert_eq!(attributes.get("allocation_id"), None);
}

// --- extract_ec2_vpc_endpoint_attributes tests ---

#[test]
fn test_extract_ec2_vpc_endpoint_attributes() {
    let group = aws_sdk_ec2::types::SecurityGroupIdentifier::builder()
        .group_id("sg-12345678")
        .build();
    let endpoint = aws_sdk_ec2::types::VpcEndpoint::builder()
        .vpc_endpoint_id("vpce-12345678")
        .vpc_endpoint_type(aws_sdk_ec2::types::VpcEndpointType::Gateway)
        .vpc_id("vpc-12345678")
        .service_name("com.amazonaws.ap-northeast-1.s3")
        .private_dns_enabled(false)
        .route_table_ids("rtb-12345678")
        .groups(group)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_vpc_endpoint_attributes(&endpoint, &mut attributes);
    assert_eq!(identifier, Some("vpce-12345678".to_string()));
    assert_eq!(
        attributes.get("vpc_endpoint_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpce-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_endpoint_type"),
        Some(&Value::Concrete(ConcreteValue::String(
            "Gateway".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("service_name"),
        Some(&Value::Concrete(ConcreteValue::String(
            "com.amazonaws.ap-northeast-1.s3".to_string()
        )))
    );
    assert_eq!(
        attributes.get("private_dns_enabled"),
        Some(&Value::Concrete(ConcreteValue::Bool(false)))
    );
    assert_eq!(
        attributes.get("route_table_ids"),
        Some(&Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("rtb-12345678".to_string()))
        ])))
    );
    assert_eq!(
        attributes.get("security_group_ids"),
        Some(&Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("sg-12345678".to_string()))
        ])))
    );
}

#[test]
fn test_extract_ec2_vpc_endpoint_attributes_minimal() {
    let endpoint = aws_sdk_ec2::types::VpcEndpoint::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_vpc_endpoint_attributes(&endpoint, &mut attributes);
    assert_eq!(identifier, None);
}

#[test]
fn test_extract_ec2_vpc_endpoint_attributes_interface() {
    let group = aws_sdk_ec2::types::SecurityGroupIdentifier::builder()
        .group_id("sg-99999999")
        .build();
    let endpoint = aws_sdk_ec2::types::VpcEndpoint::builder()
        .vpc_endpoint_id("vpce-99999999")
        .vpc_endpoint_type(aws_sdk_ec2::types::VpcEndpointType::Interface)
        .vpc_id("vpc-12345678")
        .service_name("com.amazonaws.ap-northeast-1.execute-api")
        .private_dns_enabled(true)
        .subnet_ids("subnet-12345678")
        .groups(group)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_vpc_endpoint_attributes(&endpoint, &mut attributes);
    assert_eq!(identifier, Some("vpce-99999999".to_string()));
    assert_eq!(
        attributes.get("vpc_endpoint_type"),
        Some(&Value::Concrete(ConcreteValue::String(
            "Interface".to_string()
        )))
    );
    assert_eq!(
        attributes.get("private_dns_enabled"),
        Some(&Value::Concrete(ConcreteValue::Bool(true)))
    );
    assert_eq!(
        attributes.get("subnet_ids"),
        Some(&Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("subnet-12345678".to_string()))
        ])))
    );
}

// --- extract_ec2_flow_log_attributes tests ---

#[test]
fn test_extract_ec2_flow_log_attributes() {
    let fl = aws_sdk_ec2::types::FlowLog::builder()
        .flow_log_id("fl-12345678")
        .resource_id("vpc-12345678")
        .traffic_type(aws_sdk_ec2::types::TrafficType::All)
        .log_destination_type(aws_sdk_ec2::types::LogDestinationType::S3)
        .log_destination("arn:aws:s3:::my-bucket")
        .max_aggregation_interval(600)
        .flow_log_status("ACTIVE")
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_flow_log_attributes(&fl, &mut attributes);
    assert_eq!(identifier, Some("fl-12345678".to_string()));
    assert_eq!(
        attributes.get("flow_log_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "fl-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("resource_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("traffic_type"),
        Some(&Value::Concrete(ConcreteValue::String("ALL".to_string())))
    );
    assert_eq!(
        attributes.get("log_destination_type"),
        Some(&Value::Concrete(ConcreteValue::String("s3".to_string())))
    );
    assert_eq!(
        attributes.get("log_destination"),
        Some(&Value::Concrete(ConcreteValue::String(
            "arn:aws:s3:::my-bucket".to_string()
        )))
    );
    assert_eq!(
        attributes.get("max_aggregation_interval"),
        Some(&Value::Concrete(ConcreteValue::Int(600)))
    );
    assert_eq!(
        attributes.get("resource_type"),
        Some(&Value::Concrete(ConcreteValue::String("VPC".to_string())))
    );
}

#[test]
fn test_extract_ec2_flow_log_attributes_minimal() {
    let fl = aws_sdk_ec2::types::FlowLog::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_flow_log_attributes(&fl, &mut attributes);
    assert_eq!(identifier, None);
}

#[test]
fn test_extract_ec2_flow_log_attributes_cloudwatch() {
    let fl = aws_sdk_ec2::types::FlowLog::builder()
        .flow_log_id("fl-87654321")
        .resource_id("subnet-12345678")
        .traffic_type(aws_sdk_ec2::types::TrafficType::Accept)
        .log_destination_type(aws_sdk_ec2::types::LogDestinationType::CloudWatchLogs)
        .log_group_name("/aws/vpc/flow-logs")
        .deliver_logs_permission_arn("arn:aws:iam::123456789012:role/flow-log-role")
        .flow_log_status("ACTIVE")
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_flow_log_attributes(&fl, &mut attributes);
    assert_eq!(identifier, Some("fl-87654321".to_string()));
    assert_eq!(
        attributes.get("log_group_name"),
        Some(&Value::Concrete(ConcreteValue::String(
            "/aws/vpc/flow-logs".to_string()
        )))
    );
    assert_eq!(
        attributes.get("deliver_logs_permission_arn"),
        Some(&Value::Concrete(ConcreteValue::String(
            "arn:aws:iam::123456789012:role/flow-log-role".to_string()
        )))
    );
    assert_eq!(
        attributes.get("resource_type"),
        Some(&Value::Concrete(ConcreteValue::String(
            "Subnet".to_string()
        )))
    );
}

// --- extract_ec2_vpn_gateway_attributes tests ---

#[test]
fn test_extract_ec2_vpn_gateway_attributes() {
    let vgw = aws_sdk_ec2::types::VpnGateway::builder()
        .vpn_gateway_id("vgw-12345678")
        .r#type(aws_sdk_ec2::types::GatewayType::Ipsec1)
        .amazon_side_asn(64512)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_vpn_gateway_attributes(&vgw, &mut attributes);
    assert_eq!(identifier, Some("vgw-12345678".to_string()));
    assert_eq!(
        attributes.get("vpn_gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vgw-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("type"),
        Some(&Value::Concrete(ConcreteValue::String(
            "ipsec.1".to_string()
        )))
    );
    assert_eq!(
        attributes.get("amazon_side_asn"),
        Some(&Value::Concrete(ConcreteValue::Int(64512)))
    );
}

#[test]
fn test_extract_ec2_vpn_gateway_attributes_minimal() {
    let vgw = aws_sdk_ec2::types::VpnGateway::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_vpn_gateway_attributes(&vgw, &mut attributes);
    assert_eq!(identifier, None);
    assert!(attributes.is_empty());
}

// --- extract_iam_role_attributes tests ---

#[test]
fn test_extract_iam_role_attributes() {
    let role = aws_sdk_iam::types::Role::builder()
        .role_name("test-role")
        .role_id("AROAEXAMPLE12345")
        .arn("arn:aws:iam::123456789012:role/test-role")
        .path("/")
        .assume_role_policy_document(
            "%7B%22Version%22%3A%222012-10-17%22%2C%22Statement%22%3A%5B%7B%22Effect%22%3A%22Allow%22%2C%22Principal%22%3A%7B%22Service%22%3A%22ec2.amazonaws.com%22%7D%2C%22Action%22%3A%22sts%3AAssumeRole%22%7D%5D%7D",
        )
        .description("Test role")
        .max_session_duration(7200)
        .create_date(aws_sdk_iam::primitives::DateTime::from_secs(0))
        .build()
        .expect("failed to build Role");
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_iam_role_attributes(&role, &mut attributes);
    assert_eq!(identifier, Some("test-role".to_string()));
    assert_eq!(
        attributes.get("role_name"),
        Some(&Value::Concrete(ConcreteValue::String(
            "test-role".to_string()
        )))
    );
    assert_eq!(
        attributes.get("role_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "AROAEXAMPLE12345".to_string()
        )))
    );
    assert_eq!(
        attributes.get("arn"),
        Some(&Value::Concrete(ConcreteValue::String(
            "arn:aws:iam::123456789012:role/test-role".to_string()
        )))
    );
    assert_eq!(
        attributes.get("path"),
        Some(&Value::Concrete(ConcreteValue::String("/".to_string())))
    );
    assert_eq!(
        attributes.get("description"),
        Some(&Value::Concrete(ConcreteValue::String(
            "Test role".to_string()
        )))
    );
    assert_eq!(
        attributes.get("max_session_duration"),
        Some(&Value::Concrete(ConcreteValue::Int(7200)))
    );
    // Verify that the assume_role_policy_document is converted to a Map
    // with snake_case keys. `version` lands as `EnumIdentifier` carrying
    // the underscore DSL alias (`2012_10_17`); the read-side helper maps
    // AWS's canonical `"2012-10-17"` to the alias for plan-verify parity.
    let policy_doc = attributes
        .get("assume_role_policy_document")
        .expect("assume_role_policy_document should be present");
    if let Value::Concrete(ConcreteValue::Map(map)) = policy_doc {
        assert!(map.contains_key("version"), "should have 'version' key");
        assert!(map.contains_key("statement"), "should have 'statement' key");
        if let Some(Value::Concrete(ConcreteValue::String(v))) = map.get("version") {
            assert_eq!(v, "2012-10-17");
        } else {
            panic!(
                "Expected version to be raw AWS-canonical String (aws#326), got {:?}",
                map.get("version")
            );
        }
    } else {
        panic!("Expected Map, got {:?}", policy_doc);
    }
}

#[test]
fn test_extract_iam_role_attributes_minimal() {
    let role = aws_sdk_iam::types::Role::builder()
        .role_name("minimal-role")
        .role_id("AROAMINIMAL")
        .arn("arn:aws:iam::123456789012:role/minimal-role")
        .path("/")
        .create_date(aws_sdk_iam::primitives::DateTime::from_secs(0))
        .build()
        .expect("failed to build Role");
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_iam_role_attributes(&role, &mut attributes);
    assert_eq!(identifier, Some("minimal-role".to_string()));
    assert_eq!(attributes.get("description"), None);
    assert_eq!(attributes.get("max_session_duration"), None);
}

// --- extract_ec2_transit_gateway_attributes tests ---

#[test]
fn test_extract_ec2_transit_gateway_attributes() {
    let options = aws_sdk_ec2::types::TransitGatewayOptions::builder()
        .amazon_side_asn(64512)
        .auto_accept_shared_attachments(
            aws_sdk_ec2::types::AutoAcceptSharedAttachmentsValue::Enable,
        )
        .default_route_table_association(
            aws_sdk_ec2::types::DefaultRouteTableAssociationValue::Enable,
        )
        .default_route_table_propagation(
            aws_sdk_ec2::types::DefaultRouteTablePropagationValue::Enable,
        )
        .dns_support(aws_sdk_ec2::types::DnsSupportValue::Enable)
        .vpn_ecmp_support(aws_sdk_ec2::types::VpnEcmpSupportValue::Enable)
        .build();
    let tgw = aws_sdk_ec2::types::TransitGateway::builder()
        .transit_gateway_id("tgw-12345678")
        .description("Test TGW")
        .options(options)
        .build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_transit_gateway_attributes(&tgw, &mut attributes);
    assert_eq!(identifier, Some("tgw-12345678".to_string()));
    assert_eq!(
        attributes.get("transit_gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "tgw-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("description"),
        Some(&Value::Concrete(ConcreteValue::String(
            "Test TGW".to_string()
        )))
    );
    assert_eq!(
        attributes.get("amazon_side_asn"),
        Some(&Value::Concrete(ConcreteValue::Int(64512)))
    );
    assert_eq!(
        attributes.get("auto_accept_shared_attachments"),
        Some(&Value::Concrete(ConcreteValue::String(
            "enable".to_string()
        )))
    );
    assert_eq!(
        attributes.get("dns_support"),
        Some(&Value::Concrete(ConcreteValue::String(
            "enable".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpn_ecmp_support"),
        Some(&Value::Concrete(ConcreteValue::String(
            "enable".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_transit_gateway_attributes_minimal() {
    let tgw = aws_sdk_ec2::types::TransitGateway::builder().build();
    let mut attributes = HashMap::new();
    let identifier = AwsProvider::extract_ec2_transit_gateway_attributes(&tgw, &mut attributes);
    assert_eq!(identifier, None);
}

// --- extract_ec2_transit_gateway_attachment_attributes tests ---

#[test]
fn test_extract_ec2_transit_gateway_attachment_attributes() {
    let att = aws_sdk_ec2::types::TransitGatewayVpcAttachment::builder()
        .transit_gateway_attachment_id("tgw-attach-12345678")
        .transit_gateway_id("tgw-12345678")
        .vpc_id("vpc-12345678")
        .subnet_ids("subnet-12345678")
        .subnet_ids("subnet-87654321")
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_transit_gateway_attachment_attributes(&att, &mut attributes);
    assert_eq!(identifier, Some("tgw-attach-12345678".to_string()));
    assert_eq!(
        attributes.get("transit_gateway_attachment_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "tgw-attach-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("transit_gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "tgw-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("subnet_ids"),
        Some(&Value::Concrete(ConcreteValue::List(vec![
            Value::Concrete(ConcreteValue::String("subnet-12345678".to_string())),
            Value::Concrete(ConcreteValue::String("subnet-87654321".to_string())),
        ])))
    );
}

#[test]
fn test_extract_ec2_transit_gateway_attachment_attributes_minimal() {
    let att = aws_sdk_ec2::types::TransitGatewayVpcAttachment::builder().build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_transit_gateway_attachment_attributes(&att, &mut attributes);
    assert_eq!(identifier, None);
}

// --- extract_ec2_vpc_peering_connection_attributes tests ---

#[test]
fn test_extract_ec2_vpc_peering_connection_attributes() {
    let requester = aws_sdk_ec2::types::VpcPeeringConnectionVpcInfo::builder()
        .vpc_id("vpc-11111111")
        .build();
    let accepter = aws_sdk_ec2::types::VpcPeeringConnectionVpcInfo::builder()
        .vpc_id("vpc-22222222")
        .owner_id("123456789012")
        .region("ap-northeast-1")
        .build();
    let pcx = aws_sdk_ec2::types::VpcPeeringConnection::builder()
        .vpc_peering_connection_id("pcx-12345678")
        .requester_vpc_info(requester)
        .accepter_vpc_info(accepter)
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_vpc_peering_connection_attributes(&pcx, &mut attributes);
    assert_eq!(identifier, Some("pcx-12345678".to_string()));
    assert_eq!(
        attributes.get("vpc_peering_connection_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "pcx-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-11111111".to_string()
        )))
    );
    assert_eq!(
        attributes.get("peer_vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-22222222".to_string()
        )))
    );
    assert_eq!(
        attributes.get("peer_owner_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "123456789012".to_string()
        )))
    );
    assert_eq!(
        attributes.get("peer_region"),
        Some(&Value::Concrete(ConcreteValue::String(
            "ap-northeast-1".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_vpc_peering_connection_attributes_minimal() {
    let pcx = aws_sdk_ec2::types::VpcPeeringConnection::builder().build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_vpc_peering_connection_attributes(&pcx, &mut attributes);
    assert_eq!(identifier, None);
}

// --- extract_ec2_egress_only_internet_gateway_attributes tests ---

#[test]
fn test_extract_ec2_egress_only_internet_gateway_attributes() {
    let attachment = aws_sdk_ec2::types::InternetGatewayAttachment::builder()
        .vpc_id("vpc-12345678")
        .state(aws_sdk_ec2::types::AttachmentStatus::from("attached"))
        .build();
    let eigw = aws_sdk_ec2::types::EgressOnlyInternetGateway::builder()
        .egress_only_internet_gateway_id("eigw-12345678")
        .attachments(attachment)
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_egress_only_internet_gateway_attributes(&eigw, &mut attributes);
    assert_eq!(identifier, Some("eigw-12345678".to_string()));
    assert_eq!(
        attributes.get("egress_only_internet_gateway_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "eigw-12345678".to_string()
        )))
    );
    assert_eq!(
        attributes.get("vpc_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "vpc-12345678".to_string()
        )))
    );
}

#[test]
fn test_extract_ec2_egress_only_internet_gateway_attributes_minimal() {
    let eigw = aws_sdk_ec2::types::EgressOnlyInternetGateway::builder().build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_ec2_egress_only_internet_gateway_attributes(&eigw, &mut attributes);
    assert_eq!(identifier, None);
}

// --- extract_organizations_organization_attributes tests ---

#[test]
fn test_extract_organizations_organization_attributes() {
    let org = aws_sdk_organizations::types::Organization::builder()
        .id("o-abc123")
        .arn("arn:aws:organizations::123456789012:organization/o-abc123")
        .feature_set(aws_sdk_organizations::types::OrganizationFeatureSet::All)
        .master_account_id("123456789012")
        .master_account_arn("arn:aws:organizations::123456789012:account/o-abc123/123456789012")
        .master_account_email("admin@example.com")
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_organizations_organization_attributes(&org, &mut attributes);
    assert_eq!(identifier, Some("o-abc123".to_string()));
    assert_eq!(
        attributes.get("id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "o-abc123".to_string()
        )))
    );
    assert_eq!(
        attributes.get("arn"),
        Some(&Value::Concrete(ConcreteValue::String(
            "arn:aws:organizations::123456789012:organization/o-abc123".to_string()
        )))
    );
    assert_eq!(
        attributes.get("feature_set"),
        Some(&Value::Concrete(ConcreteValue::String("ALL".to_string())))
    );
    assert_eq!(
        attributes.get("master_account_id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "123456789012".to_string()
        )))
    );
    assert_eq!(
        attributes.get("master_account_arn"),
        Some(&Value::Concrete(ConcreteValue::String(
            "arn:aws:organizations::123456789012:account/o-abc123/123456789012".to_string()
        )))
    );
    assert_eq!(
        attributes.get("master_account_email"),
        Some(&Value::Concrete(ConcreteValue::String(
            "admin@example.com".to_string()
        )))
    );
}

#[test]
fn test_extract_organizations_organization_attributes_consolidated_billing() {
    let org = aws_sdk_organizations::types::Organization::builder()
        .id("o-xyz789")
        .feature_set(aws_sdk_organizations::types::OrganizationFeatureSet::ConsolidatedBilling)
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_organizations_organization_attributes(&org, &mut attributes);
    assert_eq!(identifier, Some("o-xyz789".to_string()));
    assert_eq!(
        attributes.get("feature_set"),
        Some(&Value::Concrete(ConcreteValue::String(
            "CONSOLIDATED_BILLING".to_string()
        )))
    );
}

#[test]
fn test_extract_organizations_organization_attributes_minimal() {
    let org = aws_sdk_organizations::types::Organization::builder().build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_organizations_organization_attributes(&org, &mut attributes);
    assert_eq!(identifier, None);
    assert!(attributes.is_empty());
}

#[test]
fn test_organizations_organization_schema_feature_set_enum() {
    let config =
        crate::schemas::generated::organizations::organization::organizations_organization_config();
    let feature_set = config
        .schema
        .attributes
        .get("feature_set")
        .expect("feature_set attribute not found");
    if let carina_core::schema::Shape::Enum {
        values: Some(values),
        ..
    } = config.schema.shape_of(&feature_set.attr_type)
    {
        assert!(values.contains(&"ALL".to_string()));
        assert!(values.contains(&"CONSOLIDATED_BILLING".to_string()));
        assert_eq!(values.len(), 2);
    } else {
        panic!("feature_set should be enum");
    }
}

// --- extract_organizations_account_attributes tests ---

#[test]
fn test_extract_organizations_account_attributes() {
    let account = aws_sdk_organizations::types::Account::builder()
        .id("123456789012")
        .arn("arn:aws:organizations::111111111111:account/o-abc123/123456789012")
        .name("production")
        .email("prod@example.com")
        .status(aws_sdk_organizations::types::AccountStatus::Active)
        .joined_method(aws_sdk_organizations::types::AccountJoinedMethod::Created)
        .joined_timestamp(aws_sdk_organizations::primitives::DateTime::from_secs(
            1700000000,
        ))
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_organizations_account_attributes(&account, &mut attributes);
    assert_eq!(identifier, Some("123456789012".to_string()));
    assert_eq!(
        attributes.get("id"),
        Some(&Value::Concrete(ConcreteValue::String(
            "123456789012".to_string()
        )))
    );
    assert_eq!(
        attributes.get("arn"),
        Some(&Value::Concrete(ConcreteValue::String(
            "arn:aws:organizations::111111111111:account/o-abc123/123456789012".to_string()
        )))
    );
    assert_eq!(
        attributes.get("name"),
        Some(&Value::Concrete(ConcreteValue::String(
            "production".to_string()
        )))
    );
    assert_eq!(
        attributes.get("email"),
        Some(&Value::Concrete(ConcreteValue::String(
            "prod@example.com".to_string()
        )))
    );
    assert_eq!(
        attributes.get("status"),
        Some(&Value::Concrete(ConcreteValue::String(
            "ACTIVE".to_string()
        )))
    );
    assert_eq!(
        attributes.get("joined_method"),
        Some(&Value::Concrete(ConcreteValue::String(
            "CREATED".to_string()
        )))
    );
    assert!(attributes.contains_key("joined_timestamp"));
}

#[test]
fn test_extract_organizations_account_attributes_minimal() {
    let account = aws_sdk_organizations::types::Account::builder().build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_organizations_account_attributes(&account, &mut attributes);
    assert_eq!(identifier, None);
    assert!(attributes.is_empty());
}

#[test]
fn test_extract_organizations_account_attributes_suspended() {
    let account = aws_sdk_organizations::types::Account::builder()
        .id("999999999999")
        .status(aws_sdk_organizations::types::AccountStatus::Suspended)
        .joined_method(aws_sdk_organizations::types::AccountJoinedMethod::Invited)
        .build();
    let mut attributes = HashMap::new();
    let identifier =
        AwsProvider::extract_organizations_account_attributes(&account, &mut attributes);
    assert_eq!(identifier, Some("999999999999".to_string()));
    assert_eq!(
        attributes.get("status"),
        Some(&Value::Concrete(ConcreteValue::String(
            "SUSPENDED".to_string()
        )))
    );
    assert_eq!(
        attributes.get("joined_method"),
        Some(&Value::Concrete(ConcreteValue::String(
            "INVITED".to_string()
        )))
    );
}

#[test]
fn test_organizations_account_schema_attributes() {
    let config = crate::schemas::generated::organizations::account::organizations_account_config();
    let schema = &config.schema;
    // Verify key attributes exist
    assert!(schema.attributes.contains_key("account_name"));
    assert!(schema.attributes.contains_key("email"));
    assert!(schema.attributes.contains_key("id"));
    assert!(schema.attributes.contains_key("arn"));
    assert!(schema.attributes.contains_key("status"));
    assert!(schema.attributes.contains_key("name"));
    assert!(schema.attributes.contains_key("tags"));
    // Verify has_tags
    assert!(config.has_tags);
}

// --- ip_protocol enum "all" variant tests (issue #1428) ---

#[test]
fn test_security_group_egress_schema_includes_all_variant() {
    // The "all" value (alias for "-1") must be included in the enum values
    // so it is accepted even when to_dsl is lost during protocol serialization.
    let config =
        crate::schemas::generated::ec2::security_group_egress::ec2_security_group_egress_config();
    let ip_protocol = config
        .schema
        .attributes
        .get("ip_protocol")
        .expect("ip_protocol attribute not found");
    if let carina_core::schema::Shape::Enum {
        values: Some(values),
        ..
    } = config.schema.shape_of(&ip_protocol.attr_type)
    {
        assert!(
            values.contains(&"all".to_string()),
            "enum values must include 'all': {:?}",
            values
        );
    } else {
        panic!("ip_protocol should be enum");
    }
}

#[test]
fn test_security_group_ingress_schema_includes_all_variant() {
    let config =
        crate::schemas::generated::ec2::security_group_ingress::ec2_security_group_ingress_config();
    let ip_protocol = config
        .schema
        .attributes
        .get("ip_protocol")
        .expect("ip_protocol attribute not found");
    if let carina_core::schema::Shape::Enum {
        values: Some(values),
        ..
    } = config.schema.shape_of(&ip_protocol.attr_type)
    {
        assert!(
            values.contains(&"all".to_string()),
            "enum values must include 'all': {:?}",
            values
        );
    } else {
        panic!("ip_protocol should be enum");
    }
}

// --- Issue #247: D7 snake_case enum DSL spelling ---

/// Per naming-conventions design D7, every `StringEnum` emitted by codegen
/// must carry a `to_dsl` closure that maps each canonical AWS value to its
/// snake_case DSL alias. The validator's `matches_alias` path then accepts
/// the snake_case form alongside the canonical form.
#[test]
fn test_organization_feature_set_validates_snake_case_alias() {
    use carina_core::resource::{ConcreteValue, Value};

    let config =
        crate::schemas::generated::organizations::organization::organizations_organization_config();
    let feature_set = config
        .schema
        .attributes
        .get("feature_set")
        .expect("feature_set attribute not found");

    // Phase 4 of carina#2986: enum attributes accept only
    // `EnumIdentifier` shape; a `ConcreteValue::String` here would route
    // to `StringLiteralExpectedEnum`. Both the API form (`ALL`) and the
    // DSL form (`all`) are identifier-shape inputs in real DSL — the
    // parser would emit `EnumIdentifier` for both. Mirror that here.
    let schema = carina_core::schema::Schema::flat(feature_set.attr_type.clone());
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier("ALL")))
        .expect("API spelling ALL should be accepted");
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier("all")))
        .expect("DSL spelling all should be accepted");
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier(
            "CONSOLIDATED_BILLING",
        )))
        .expect("API spelling CONSOLIDATED_BILLING should be accepted");
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier(
            "consolidated_billing",
        )))
        .expect("DSL spelling consolidated_billing should be accepted");
    // Bogus values still rejected.
    assert!(
        schema
            .validate(&Value::Concrete(ConcreteValue::enum_identifier(
                "not_a_value"
            )))
            .is_err(),
        "unknown values must still be rejected"
    );
}

/// Sibling check for a PascalCase StringEnum: route53.RecordSet's `Type`
/// should accept both `A`, `AAAA`, ... (the API spellings, which happen to
/// be SHOUTY) and their lowercase DSL aliases.
#[test]
fn test_route53_record_type_validates_snake_case_alias() {
    use carina_core::resource::{ConcreteValue, Value};

    let config = crate::schemas::generated::route53::record_set::route53_record_set_config();
    let type_attr = config
        .schema
        .attributes
        .get("type")
        .expect("type attribute not found");

    // Phase 4 of carina#2986: use `EnumIdentifier` shape to reach the
    // variant-match path. A `ConcreteValue::String` here would route
    // to `StringLiteralExpectedEnum`.
    let schema = carina_core::schema::Schema::flat(type_attr.attr_type.clone());
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier("A")))
        .expect("API spelling A should be accepted");
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier("a")))
        .expect("DSL spelling a should be accepted");
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier("CNAME")))
        .expect("API spelling CNAME should be accepted");
    schema
        .validate(&Value::Concrete(ConcreteValue::enum_identifier("cname")))
        .expect("DSL spelling cname should be accepted");
}
