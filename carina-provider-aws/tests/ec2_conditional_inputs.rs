use carina_core::provider::ProviderNormalizer;
use carina_core::resource::{ConcreteValue, DeferredValue, ProviderResource, Resource, Value};
use carina_provider_aws::AwsNormalizer;
use indexmap::IndexMap;

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}

fn int(value: i64) -> Value {
    Value::Concrete(ConcreteValue::Int(value))
}

fn bool_(value: bool) -> Value {
    Value::Concrete(ConcreteValue::Bool(value))
}

fn map(values: &[(&str, Value)]) -> Value {
    Value::Concrete(ConcreteValue::Map(
        values
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<IndexMap<_, _>>(),
    ))
}

async fn normalize(resource: &str, attrs: &[(&str, Value)]) -> Result<ProviderResource, String> {
    let mut resource = Resource::with_provider("aws", format!("ec2.{resource}"), "ec2-rules", None);
    for (key, value) in attrs {
        resource.set_attr(*key, value.clone());
    }
    let mut resources = vec![resource.into()];
    AwsNormalizer
        .normalize_desired(&mut resources)
        .await
        .map_err(|error| error.to_string())?;
    Ok(resources.remove(0))
}

fn flow(resource_type: Value) -> Vec<(&'static str, Value)> {
    vec![
        ("resource_type", resource_type),
        ("log_destination_type", string("s3")),
        (
            "log_destination",
            string("arn:aws:s3:::carina-flow-logs/prefix/"),
        ),
    ]
}

#[tokio::test]
async fn ec2_flow_log_transit_gateway_rules_are_decidable_from_config() {
    // ec2.json CreateFlowLogsRequest${MaxAggregationInterval,TrafficType,ResourceIds}.
    for resource_type in ["TransitGateway", "TransitGatewayAttachment"] {
        let mut attrs = flow(string(resource_type));
        attrs.push(("max_aggregation_interval", int(600)));
        let error = normalize("FlowLog", &attrs).await.unwrap_err();
        assert!(error.contains("max_aggregation_interval"), "{error}");
        attrs.pop();
        attrs.push(("max_aggregation_interval", int(60)));
        assert!(normalize("FlowLog", &attrs).await.is_ok());
        attrs.push(("traffic_type", string("ALL")));
        let error = normalize("FlowLog", &attrs).await.unwrap_err();
        assert!(error.contains("traffic_type"), "{error}");
    }
    for resource_type in ["VPC", "Subnet", "NetworkInterface", "RegionalNatGateway"] {
        let mut attrs = flow(string(resource_type));
        let error = normalize("FlowLog", &attrs).await.unwrap_err();
        assert!(error.contains("traffic_type"), "{error}");
        attrs.push(("traffic_type", string("ALL")));
        attrs.push(("max_aggregation_interval", int(600)));
        assert!(normalize("FlowLog", &attrs).await.is_ok());
    }
}

#[tokio::test]
async fn ec2_flow_log_resource_count_depends_on_resource_type() {
    for (resource_type, maximum) in [
        ("TransitGateway", 25),
        ("TransitGatewayAttachment", 25),
        ("VPC", 300),
    ] {
        let mut attrs = flow(string(resource_type));
        if resource_type == "VPC" {
            attrs.push(("traffic_type", string("ALL")));
        }
        attrs.push((
            "resource_ids",
            Value::Concrete(ConcreteValue::List(vec![
                string("resource-reference");
                maximum
            ])),
        ));
        assert!(normalize("FlowLog", &attrs).await.is_ok());
        attrs.last_mut().unwrap().1 =
            Value::Concrete(ConcreteValue::List(vec![
                string("resource-reference");
                maximum + 1
            ]));
        let error = normalize("FlowLog", &attrs).await.unwrap_err();
        assert!(error.contains("resource_ids"), "{error}");
    }
}

#[tokio::test]
async fn ec2_flow_log_cloudwatch_role_and_group_rules_use_destination_default() {
    for destination in [None, Some("cloud-watch-logs")] {
        let mut attrs = vec![
            ("resource_type", string("VPC")),
            ("traffic_type", string("ALL")),
            ("log_group_name", string("aws/existing")),
        ];
        if let Some(destination) = destination {
            attrs.push(("log_destination_type", string(destination)));
        }
        let error = normalize("FlowLog", &attrs).await.unwrap_err();
        assert!(error.contains("deliver_logs_permission_arn"), "{error}");
        attrs.push((
            "deliver_logs_permission_arn",
            string("arn:aws:iam::123456789012:role/FlowLogs"),
        ));
        assert!(normalize("FlowLog", &attrs).await.is_ok());
    }
    let mut attrs = flow(string("TransitGateway"));
    attrs.push(("log_group_name", string("/carina/flow-logs")));
    let error = normalize("FlowLog", &attrs).await.unwrap_err();
    assert!(error.contains("log_group_name"), "{error}");
}

#[tokio::test]
async fn ec2_subnet_ipv6_only_rejects_ipv4_allocation_and_ip_name() {
    // CreateSubnet and ModifySubnetAttribute are authoritative for requests.
    for (field, value) in [
        ("cidr_block", string("10.0.0.0/24")),
        ("ipv4_ipam_pool_id", string("ipam-pool-12345678")),
        ("ipv4_netmask_length", int(24)),
        (
            "private_dns_name_options_on_launch",
            map(&[("hostname_type", string("ip-name"))]),
        ),
    ] {
        let attrs = [
            ("ipv6_native", bool_(true)),
            ("ipv6_cidr_block", string("2001:db8::/64")),
            (field, value),
        ];
        let error = normalize("Subnet", &attrs).await.unwrap_err();
        assert!(error.contains(field), "{error}");
    }
    let attrs = [
        ("ipv6_native", bool_(true)),
        ("ipv6_cidr_block", string("2001:db8::/64")),
        (
            "private_dns_name_options_on_launch",
            map(&[("hostname_type", string("resource-name"))]),
        ),
    ];
    assert!(normalize("Subnet", &attrs).await.is_ok());
    // ModifySubnetAttribute explicitly permits resource-name for IPv4-only;
    // PrivateDnsNameOptionsOnLaunch's older prose disagrees.
    assert!(
        normalize(
            "Subnet",
            &[
                ("cidr_block", string("10.0.0.0/24")),
                (
                    "private_dns_name_options_on_launch",
                    map(&[("hostname_type", string("resource-name"))])
                )
            ]
        )
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn ec2_vpc_rejects_create_only_host_tenancy_without_restricting_read_enum() {
    // carina-test-004, 2026-11-05: CreateVpc rejects host with
    // InvalidParameterValue, confirming the request-only restriction.
    let error = normalize("Vpc", &[("instance_tenancy", string("host"))])
        .await
        .unwrap_err();
    assert!(error.contains("instance_tenancy"), "{error}");
    for tenancy in ["default", "dedicated"] {
        assert!(
            normalize("Vpc", &[("instance_tenancy", string(tenancy))])
                .await
                .is_ok()
        );
    }
}

#[tokio::test]
async fn ec2_rules_preserve_and_defer_unknown_secrets_references_and_nested_parents() {
    let reference = Value::resource_ref("peer", "attribute", vec![]).resolve_root_for_test();
    for unknown in [
        reference,
        Value::Deferred(DeferredValue::Secret(Box::new(string("unresolved")))),
    ] {
        let mut attrs = flow(unknown.clone());
        attrs.push(("max_aggregation_interval", int(600)));
        let resource = normalize("FlowLog", &attrs).await.unwrap();
        assert_eq!(resource.get_attr("resource_type"), Some(&unknown));
        let mut attrs = flow(string("TransitGateway"));
        attrs.push(("max_aggregation_interval", unknown.clone()));
        attrs.push(("traffic_type", unknown.clone()));
        attrs.push(("resource_ids", unknown.clone()));
        assert!(normalize("FlowLog", &attrs).await.is_ok());
        let attrs = [
            ("ipv6_native", bool_(true)),
            ("cidr_block", unknown.clone()),
            ("private_dns_name_options_on_launch", unknown.clone()),
        ];
        let resource = normalize("Subnet", &attrs).await.unwrap();
        assert_eq!(resource.get_attr("cidr_block"), Some(&unknown));
        let attrs = [
            ("ipv6_native", unknown.clone()),
            ("cidr_block", string("10.0.0.0/24")),
        ];
        assert!(normalize("Subnet", &attrs).await.is_ok());
        assert!(
            normalize("Vpc", &[("instance_tenancy", unknown)])
                .await
                .is_ok()
        );
    }
}

#[tokio::test]
async fn ec2_rules_keep_existing_cidr_and_name_spellings() {
    let resource = normalize("Vpc", &[("cidr_block", string("100.68.0.18/18"))])
        .await
        .unwrap();
    assert_eq!(
        resource.get_attr("cidr_block"),
        Some(&string("100.68.0.18/18"))
    );
    let resource = normalize(
        "SecurityGroup",
        &[("group_name", string("Carina Mixed Case"))],
    )
    .await
    .unwrap();
    assert_eq!(
        resource.get_attr("group_name"),
        Some(&string("Carina Mixed Case"))
    );
}
