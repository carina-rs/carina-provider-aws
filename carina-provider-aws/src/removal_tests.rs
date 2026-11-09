//! Update removals must reach the real SDK request serializer, without AWS.

use std::collections::HashMap;

use carina_core::provider::{AppliedIdentifier, PatchOp, ProviderError, UpdatePatch};
use carina_core::resource::{ConcreteValue, ResourceId, SnapshotState, Value};
use indexmap::IndexMap;

use crate::helpers::UpdateResource;
use crate::test_http::provider;

fn boolean(value: bool) -> Value {
    Value::Concrete(ConcreteValue::Bool(value))
}

fn integer(value: i64) -> Value {
    Value::Concrete(ConcreteValue::Int(value))
}

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}

fn map(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Concrete(ConcreteValue::Map(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect::<IndexMap<_, _>>(),
    ))
}

fn snapshot(
    resource_type: &str,
    fields: impl IntoIterator<Item = (&'static str, Value)>,
) -> SnapshotState {
    crate::read_model::test_existing_state(
        ResourceId::with_provider_identity("aws", resource_type, "removal", None),
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect::<HashMap<_, _>>(),
    )
    .to_snapshot()
}

fn remove(from: &SnapshotState, key: &str) -> UpdateResource {
    UpdateResource::new(
        from,
        &UpdatePatch {
            ops: vec![PatchOp::Remove { key: key.into() }],
        },
    )
}

fn ec2_response(action: &str) -> String {
    format!(
        "<{action}Response xmlns=\"http://ec2.amazonaws.com/doc/2016-11-15/\"><return>true</return></{action}Response>"
    )
}

fn sqs_missing_queue_response() -> (u16, String) {
    (
        400,
        r#"{"__type":"QueueDoesNotExist","message":"scripted read"}"#.into(),
    )
}

fn query_parameters(request: &str) -> HashMap<String, String> {
    request
        .split_once("; body=")
        .unwrap()
        .1
        .split('&')
        .map(|parameter| {
            let (key, value) = parameter.split_once('=').unwrap();
            (
                key.to_string(),
                urlencoding::decode(value).unwrap().into_owned(),
            )
        })
        .collect()
}

fn assert_removal_rejected(
    error: ProviderError,
    attribute: &str,
) -> carina_core::provider::HostError {
    assert!(matches!(error, ProviderError::InvalidInput(_)));
    let error = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
    assert!(error.to_string().contains(attribute), "{error}");
    // Core's typed diagnostic predicate cannot capture an expected value.
    // Keep it local to this thread while inspecting the cause synchronously.
    thread_local! {
        static EXPECTED_ATTRIBUTE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    EXPECTED_ATTRIBUTE.with(|expected| attribute.clone_into(&mut expected.borrow_mut()));
    let cause = error.detail().cause().expect("typed removal cause");
    assert!(cause.is::<crate::helpers::AttributeRemovalError>());
    assert!(
        cause
            .as_cause()
            .matches_for_test::<crate::helpers::AttributeRemovalError>(|error| {
                EXPECTED_ATTRIBUTE.with(|expected| error.attribute == *expected.borrow())
                    && !error.reason.is_empty()
            }),
        "{error}"
    );
    error
}

#[test]
fn removal_error_explains_how_to_choose_a_value() {
    let id = snapshot("sqs.Queue", []).id;
    let reason = "The API does not document a reset matching create-time omission";
    let cause = crate::helpers::AttributeRemovalError {
        attribute: "policy".into(),
        reason,
    };
    let message = format!(
        "removing `policy` is not supported: {reason}; set it explicitly to the value you want, or replace the resource"
    );
    assert_eq!(cause.to_string(), message);
    let error = assert_removal_rejected(
        crate::helpers::removal_error(&id, "policy", reason),
        "policy",
    );
    assert!(error.to_string().contains(&message), "{error}");
    assert!(
        !error
            .to_string()
            .contains("attribute removal is not supported"),
        "{error}"
    );
}

#[test]
fn update_decoder_view_rejects_access_to_another_attribute() {
    use crate::send_model::SendResource;
    for fields in [vec![], vec![("enable_dns_support", boolean(true))]] {
        let from = snapshot("ec2.Vpc", fields);
        let to = UpdateResource::new(&from, &UpdatePatch { ops: vec![] });
        let expected = from.attributes.contains_key("enable_dns_support");
        crate::send_model::decode_change(&to, "enable_dns_support", |resource| {
            assert_eq!(resource.get_attr("enable_dns_support")?.is_some(), expected);
            assert!(matches!(
                resource.get_attr("enable_dns_hostnames"),
                Err(ProviderError::Internal(_))
            ));
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn attribute_change_resolve_only_runs_the_removal_policy_for_removed() {
    use crate::helpers::AttributeChange;
    for change in [
        AttributeChange::Set(Some(7)),
        AttributeChange::Unchanged(Some(7)),
        AttributeChange::Unchanged(None),
    ] {
        let expected = if matches!(change, AttributeChange::Unchanged(None)) {
            None
        } else {
            Some(7)
        };
        assert_eq!(
            change
                .resolve(|| panic!("an unchanged or set value needs no removal policy"))
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        AttributeChange::Removed.resolve(|| Ok(Some(300))).unwrap(),
        Some(300)
    );
    assert!(matches!(
        AttributeChange::<Option<i32>>::Removed
            .resolve(|| Err(ProviderError::invalid_input("reset unavailable"))),
        Err(ProviderError::InvalidInput(_))
    ));
}

#[test]
fn typed_changes_preserve_removal_and_lazy_snapshot_inputs() {
    use crate::helpers::AttributeChange;
    let from = snapshot(
        "ec2.Vpc",
        [
            ("enable_dns_hostnames", boolean(true)),
            ("cidr_block", string("10.0.0.0/16")),
        ],
    );
    let to = UpdateResource::new(
        &from,
        &UpdatePatch {
            ops: vec![
                PatchOp::Remove {
                    key: "enable_dns_hostnames".into(),
                },
                PatchOp::Remove { key: "tags".into() },
                PatchOp::Add {
                    key: "enable_dns_support".into(),
                    value: boolean(true),
                },
            ],
        },
    );
    assert!(matches!(
        to.attribute_change("enable_dns_hostnames").unwrap(),
        AttributeChange::Removed
    ));
    assert!(matches!(
        to.attribute_change("tags").unwrap(),
        AttributeChange::Removed
    ));
    assert!(matches!(
        to.attribute_change("instance_tenancy").unwrap(),
        AttributeChange::Unchanged(None)
    ));
    assert_eq!(
        to.materialized_attributes(),
        HashMap::from([("enable_dns_support".into(), boolean(true))])
    );
    let inputs = to.snapshot_input_attributes(crate::test_resource_schema("ec2.Vpc"));
    assert!(!inputs.contains_key("enable_dns_hostnames"));
    assert!(!inputs.contains_key("tags"));
    assert_eq!(inputs.get("cidr_block"), from.attributes.get("cidr_block"));
    assert!(matches!(
        to.attribute_change("enable_dns_support").unwrap(),
        AttributeChange::Set(Some(_))
    ));
    assert!(matches!(
        to.attribute_change("cidr_block").unwrap(),
        AttributeChange::Unchanged(Some(_))
    ));
    assert_eq!(to.materialized_attributes().len(), 2);
}

#[tokio::test]
async fn subnet_removals_send_declared_resets() {
    for (key, old, ipv6_native, parameters) in [
        (
            "assign_ipv6_address_on_creation",
            boolean(true),
            false,
            vec!["AssignIpv6AddressOnCreation.Value=false"],
        ),
        (
            "assign_ipv6_address_on_creation",
            boolean(true),
            true,
            vec!["AssignIpv6AddressOnCreation.Value=true"],
        ),
        (
            "enable_dns64",
            boolean(true),
            false,
            vec!["EnableDns64.Value=false"],
        ),
        (
            "enable_lni_at_device_index",
            integer(1),
            false,
            vec!["DisableLniAtDeviceIndex.Value=true"],
        ),
        (
            "map_public_ip_on_launch",
            boolean(true),
            false,
            vec!["MapPublicIpOnLaunch.Value=false"],
        ),
        (
            "private_dns_name_options_on_launch",
            map([
                ("hostname_type", string("resource-name")),
                ("enable_resource_name_dns_a_record", boolean(true)),
                ("enable_resource_name_dns_aaaa_record", boolean(true)),
            ]),
            false,
            vec![
                "PrivateDnsHostnameTypeOnLaunch=ip-name",
                "EnableResourceNameDnsARecordOnLaunch.Value=false",
                "EnableResourceNameDnsAAAARecordOnLaunch.Value=false",
            ],
        ),
    ] {
        let from = snapshot(
            "ec2.Subnet",
            [(key, old), ("ipv6_native", boolean(ipv6_native))],
        );
        let (provider, http) = provider(
            parameters
                .iter()
                .map(|_| (200, ec2_response("ModifySubnetAttribute")))
                .chain([(200, ec2_response("DescribeSubnets"))]),
        );
        provider
            .update_ec2_subnet(
                from.id.clone(),
                &AppliedIdentifier::new("subnet-test").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("ec2.Subnet"),
            )
            .await
            .expect("removal mutation must succeed")
            .expect("scripted read");
        let requests = http.requests();
        for (request, parameter) in requests.iter().zip(&parameters) {
            assert!(
                request.contains("action=ModifySubnetAttribute;"),
                "{key}: {request}"
            );
            assert!(
                request.contains(parameter),
                "{key}: expected {parameter}: {request}"
            );
            assert!(request.contains("SubnetId=subnet-test"), "{request}");
        }
        assert!(requests.last().unwrap().contains("action=DescribeSubnets;"));
        http.assert_finished();
    }
}

#[tokio::test]
async fn subnet_hostname_reset_decodes_historical_context_only_when_needed() {
    use carina_core::resource::DeferredValue;
    for (removed, needs_context) in [
        ("private_dns_name_options_on_launch", true),
        ("map_public_ip_on_launch", false),
    ] {
        let mut from = snapshot(
            "ec2.Subnet",
            [
                (
                    "private_dns_name_options_on_launch",
                    map([("hostname_type", string("resource-name"))]),
                ),
                ("map_public_ip_on_launch", boolean(true)),
            ],
        );
        from.attributes.insert(
            "ipv6_native".into(),
            carina_core::value::redact_secrets_in_value(&Value::Deferred(DeferredValue::Secret(
                Box::new(boolean(true)).into(),
            )))
            .unwrap(),
        );
        let responses = if needs_context {
            vec![]
        } else {
            vec![
                (200, ec2_response("ModifySubnetAttribute")),
                (200, ec2_response("ModifySubnetAttribute")),
                (200, ec2_response("DescribeSubnets")),
            ]
        };
        let (provider, http) = provider(responses);
        let result = provider
            .update_ec2_subnet(
                from.id.clone(),
                &AppliedIdentifier::new("subnet-test").unwrap(),
                &from,
                &remove(&from, removed),
                crate::test_resource_schema("ec2.Subnet"),
            )
            .await;
        if needs_context {
            let error =
                result.expect_err("an unavailable IPv6 mode cannot choose a hostname reset");
            assert!(matches!(error, ProviderError::InvalidInput(_)));
            let error =
                carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
            assert!(error.detail().cause().is_some_and(|cause| {
                cause.is::<carina_core::resource::UnavailableSecretError>()
            }));
            assert!(http.requests().is_empty());
        } else {
            result
                .expect("unrelated removal must leave the historical mode unconsumed")
                .expect("scripted read");
            assert!(http.requests()[0].contains("MapPublicIpOnLaunch.Value=false"));
        }
        http.assert_finished();
    }
}

#[tokio::test]
async fn subnet_mode_dependent_removals_reject_unknown_ipv6_mode() {
    for (key, old) in [
        ("assign_ipv6_address_on_creation", boolean(true)),
        (
            "private_dns_name_options_on_launch",
            map([("hostname_type", string("resource-name"))]),
        ),
    ] {
        let from = snapshot("ec2.Subnet", [(key, old)]);
        let (provider, http) = provider([]);
        let error = provider
            .update_ec2_subnet(
                from.id.clone(),
                &AppliedIdentifier::new("subnet-test").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("ec2.Subnet"),
            )
            .await
            .expect_err("a mode-dependent reset requires a known IPv6 mode");
        let error = assert_removal_rejected(error, key);
        let cause = error
            .detail()
            .cause()
            .expect("unknown subnet mode must have a typed removal cause")
            .as_cause();
        assert!(
            cause.matches_for_test::<crate::helpers::AttributeRemovalError>(|error| error.reason
                == "Its reset depends on ipv6_native, which is not known in state")
        );
        assert!(http.requests().is_empty());
        http.assert_finished();
    }
}

#[tokio::test]
async fn subnet_nested_hostname_removal_reports_the_removed_field() {
    let from = snapshot(
        "ec2.Subnet",
        [(
            "private_dns_name_options_on_launch",
            map([("hostname_type", string("resource-name"))]),
        )],
    );
    let to = UpdateResource::new(
        &from,
        &UpdatePatch {
            ops: vec![PatchOp::Replace {
                key: "private_dns_name_options_on_launch".into(),
                value: map([]),
            }],
        },
    );
    let (provider, http) = provider([]);
    let error = provider
        .update_ec2_subnet(
            from.id.clone(),
            &AppliedIdentifier::new("subnet-test").unwrap(),
            &from,
            &to,
            crate::test_resource_schema("ec2.Subnet"),
        )
        .await
        .expect_err("hostname reset requires a known IPv6 mode");
    assert_removal_rejected(error, "private_dns_name_options_on_launch.hostname_type");
    assert!(http.requests().is_empty());
    http.assert_finished();
}

#[tokio::test]
async fn subnet_nested_removals_send_declared_resets() {
    for (field, old, parameter) in [
        (
            "hostname_type",
            string("resource-name"),
            "PrivateDnsHostnameTypeOnLaunch=ip-name",
        ),
        (
            "enable_resource_name_dns_a_record",
            boolean(true),
            "EnableResourceNameDnsARecordOnLaunch.Value=false",
        ),
        (
            "enable_resource_name_dns_aaaa_record",
            boolean(true),
            "EnableResourceNameDnsAAAARecordOnLaunch.Value=false",
        ),
    ] {
        let from = snapshot(
            "ec2.Subnet",
            [
                ("private_dns_name_options_on_launch", map([(field, old)])),
                ("ipv6_native", boolean(false)),
            ],
        );
        let to = UpdateResource::new(
            &from,
            &UpdatePatch {
                ops: vec![PatchOp::Replace {
                    key: "private_dns_name_options_on_launch".into(),
                    value: map([]),
                }],
            },
        );
        let (provider, http) = provider([
            (200, ec2_response("ModifySubnetAttribute")),
            (200, ec2_response("DescribeSubnets")),
        ]);
        provider
            .update_ec2_subnet(
                from.id.clone(),
                &AppliedIdentifier::new("subnet-test").unwrap(),
                &from,
                &to,
                crate::test_resource_schema("ec2.Subnet"),
            )
            .await
            .expect("nested removal mutation")
            .expect("scripted read");
        assert!(http.requests()[0].contains("action=ModifySubnetAttribute;"));
        assert!(
            http.requests()[0].contains(parameter),
            "{}",
            http.requests()[0]
        );
        http.assert_finished();
    }
}

#[tokio::test]
async fn vpc_removals_send_declared_resets() {
    for (key, old, parameter) in [
        (
            "enable_dns_hostnames",
            true,
            "EnableDnsHostnames.Value=false",
        ),
        ("enable_dns_support", false, "EnableDnsSupport.Value=true"),
    ] {
        let from = snapshot("ec2.Vpc", [(key, boolean(old))]);
        let (provider, http) = provider([
            (200, ec2_response("ModifyVpcAttribute")),
            (200, ec2_response("DescribeVpcs")),
        ]);
        provider
            .update_ec2_vpc(
                from.id.clone(),
                &AppliedIdentifier::new("vpc-test").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("ec2.Vpc"),
            )
            .await
            .expect("removal mutation")
            .expect("scripted read");
        assert!(http.requests()[0].contains("action=ModifyVpcAttribute;"));
        assert!(
            http.requests()[0].contains(parameter),
            "{}",
            http.requests()[0]
        );
        assert!(http.requests()[0].contains("VpcId=vpc-test"));
        http.assert_finished();
    }
}

#[tokio::test]
async fn vpc_tag_removal_deletes_the_old_keys() {
    let from = snapshot(
        "ec2.Vpc",
        [(
            "tags",
            map([("Name", string("test")), ("Environment", string("test"))]),
        )],
    );
    let (provider, http) = provider([
        (200, ec2_response("DeleteTags")),
        (200, ec2_response("DescribeVpcs")),
    ]);
    provider
        .update_ec2_vpc(
            from.id.clone(),
            &AppliedIdentifier::new("vpc-test").unwrap(),
            &from,
            &remove(&from, "tags"),
            crate::test_resource_schema("ec2.Vpc"),
        )
        .await
        .expect("tag removal")
        .expect("scripted read");
    let requests = http.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].contains("action=DeleteTags;"),
        "{}",
        requests[0]
    );
    let parameters = query_parameters(&requests[0]);
    assert_eq!(parameters["ResourceId.1"], "vpc-test");
    let keys = parameters
        .iter()
        .filter(|(key, _)| key.starts_with("Tag.") && key.ends_with(".Key"))
        .map(|(_, value)| value.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from(["Environment", "Name"])
    );
    assert!(
        !parameters
            .keys()
            .any(|key| key.starts_with("Tag.") && key.ends_with(".Value"))
    );
    assert!(
        requests[1].contains("action=DescribeVpcs;"),
        "{}",
        requests[1]
    );
    http.assert_finished();
}

#[tokio::test]
async fn transit_gateway_description_removal_sends_an_empty_description() {
    let from = snapshot(
        "ec2.TransitGateway",
        [("description", string("old description"))],
    );
    let (provider, http) = provider([
        (200, ec2_response("ModifyTransitGateway")),
        (200, ec2_response("DescribeTransitGateways")),
    ]);
    provider
        .update_ec2_transit_gateway(
            from.id.clone(),
            &AppliedIdentifier::new("tgw-test").unwrap(),
            &from,
            &remove(&from, "description"),
            crate::test_resource_schema("ec2.TransitGateway"),
        )
        .await
        .expect("description removal")
        .expect("scripted read");
    let requests = http.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].contains("action=ModifyTransitGateway;"),
        "{}",
        requests[0]
    );
    let parameters = query_parameters(&requests[0]);
    assert_eq!(parameters["TransitGatewayId"], "tgw-test");
    assert_eq!(parameters.get("Description").map(String::as_str), Some(""));
    assert!(
        requests[1].contains("action=DescribeTransitGateways;"),
        "{}",
        requests[1]
    );
    http.assert_finished();
}

#[tokio::test]
async fn vpc_endpoint_removals_send_declared_resets() {
    for (key, old, parameter) in [(
        "policy_document",
        string(r#"{"Version":"2012-10-17","Statement":[]}"#),
        "ResetPolicy=true",
    )] {
        let from = snapshot(
            "ec2.VpcEndpoint",
            [(key, old), ("vpc_endpoint_type", string("Gateway"))],
        );
        let (provider, http) = provider([
            (200, ec2_response("ModifyVpcEndpoint")),
            (200, ec2_response("DescribeVpcEndpoints")),
        ]);
        provider
            .update_ec2_vpc_endpoint(
                from.id.clone(),
                &AppliedIdentifier::new("vpce-test").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("ec2.VpcEndpoint"),
            )
            .await
            .expect("removal mutation")
            .expect("scripted read");
        assert!(http.requests()[0].contains("action=ModifyVpcEndpoint;"));
        assert!(
            http.requests()[0].contains(parameter),
            "{}",
            http.requests()[0]
        );
        assert!(!http.requests()[0].contains("PolicyDocument="));
        assert!(http.requests()[0].contains("VpcEndpointId=vpce-test"));
        http.assert_finished();
    }
}

#[tokio::test]
async fn vpc_endpoint_private_dns_removal_is_not_removable_without_requests() {
    for endpoint_type in [None, Some("Gateway"), Some("Interface")] {
        for enabled in [false, true] {
            let from = snapshot(
                "ec2.VpcEndpoint",
                [("private_dns_enabled", boolean(enabled))]
                    .into_iter()
                    .chain(endpoint_type.map(|value| ("vpc_endpoint_type", string(value)))),
            );
            let (provider, http) = provider([]);
            let error = provider
                .update_ec2_vpc_endpoint(
                    from.id.clone(),
                    &AppliedIdentifier::new("vpce-test").unwrap(),
                    &from,
                    &remove(&from, "private_dns_enabled"),
                    crate::test_resource_schema("ec2.VpcEndpoint"),
                )
                .await
                .expect_err("private DNS has no documented create-omission reset");
            assert_removal_rejected(error, "private_dns_enabled");
            assert!(http.requests().is_empty());
            http.assert_finished();
        }
    }
}

#[tokio::test]
async fn vpc_endpoint_removals_reject_undocumented_endpoint_types() {
    for (key, old, endpoint_type) in [
        (
            "policy_document",
            string(r#"{"Version":"2012-10-17","Statement":[]}"#),
            "Interface",
        ),
        ("private_dns_enabled", boolean(false), "Gateway"),
    ] {
        let from = snapshot(
            "ec2.VpcEndpoint",
            [(key, old), ("vpc_endpoint_type", string(endpoint_type))],
        );
        let (provider, http) = provider([]);
        let error = provider
            .update_ec2_vpc_endpoint(
                from.id.clone(),
                &AppliedIdentifier::new("vpce-test").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("ec2.VpcEndpoint"),
            )
            .await
            .expect_err("the reset must be documented for this endpoint type");
        assert_removal_rejected(error, key);
        assert!(http.requests().is_empty());
        http.assert_finished();
    }
}

#[tokio::test]
async fn vpc_endpoint_policy_removal_reports_the_removed_attribute_when_type_is_unknown() {
    let from = snapshot(
        "ec2.VpcEndpoint",
        [
            (
                "policy_document",
                string(r#"{"Version":"2012-10-17","Statement":[]}"#),
            ),
            ("vpc_endpoint_type", string("Gateway")),
        ],
    );
    let to = UpdateResource::new(
        &from,
        &UpdatePatch {
            ops: vec![
                PatchOp::Remove {
                    key: "policy_document".into(),
                },
                PatchOp::Remove {
                    key: "vpc_endpoint_type".into(),
                },
            ],
        },
    );
    let (provider, http) = provider([]);
    let error = provider
        .update_ec2_vpc_endpoint(
            from.id.clone(),
            &AppliedIdentifier::new("vpce-test").unwrap(),
            &from,
            &to,
            crate::test_resource_schema("ec2.VpcEndpoint"),
        )
        .await
        .expect_err("policy reset requires a known endpoint type");
    let error = assert_removal_rejected(error, "policy_document");
    assert!(
        error
            .detail()
            .cause()
            .unwrap()
            .as_cause()
            .matches_for_test::<crate::helpers::AttributeRemovalError>(|error| error.reason
                == "Its reset depends on vpc_endpoint_type, which is not known in state")
    );
    assert!(http.requests().is_empty());
    http.assert_finished();
}

#[tokio::test]
async fn log_group_removals_send_declared_requests() {
    for (key, old, action, parameter) in [
        (
            "deletion_protection_enabled",
            boolean(true),
            "PutLogGroupDeletionProtection",
            Some("\"deletionProtectionEnabled\":false"),
        ),
        (
            "retention_in_days",
            integer(7),
            "DeleteRetentionPolicy",
            None,
        ),
    ] {
        let from = snapshot("logs.LogGroup", [(key, old)]);
        let (provider, http) = provider([(200, "{}".into()), (200, r#"{"logGroups":[]}"#.into())]);
        provider
            .update_logs_log_group(
                from.id.clone(),
                &AppliedIdentifier::new("removal").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("logs.LogGroup"),
            )
            .await
            .expect("removal mutation")
            .expect("scripted read");
        assert!(
            http.requests()[0].contains(&format!(".{action};")),
            "{}",
            http.requests()[0]
        );
        if let Some(parameter) = parameter {
            assert!(
                http.requests()[0].contains(parameter),
                "{}",
                http.requests()[0]
            );
        }
        http.assert_finished();
    }
}

#[tokio::test]
async fn queue_removals_send_documented_defaults() {
    for (key, old, wire_key, reset) in [
        ("visibility_timeout", integer(60), "VisibilityTimeout", "30"),
        (
            "message_retention_period",
            integer(600),
            "MessageRetentionPeriod",
            "345600",
        ),
        ("delay_seconds", integer(60), "DelaySeconds", "0"),
        (
            "maximum_message_size",
            integer(2048),
            "MaximumMessageSize",
            "1048576",
        ),
        (
            "receive_message_wait_time_seconds",
            integer(5),
            "ReceiveMessageWaitTimeSeconds",
            "0",
        ),
        (
            "content_based_deduplication",
            boolean(true),
            "ContentBasedDeduplication",
            "false",
        ),
    ] {
        let from = snapshot(
            "sqs.Queue",
            [
                (key, old),
                ("fifo_queue", boolean(true)),
                ("kms_master_key_id", string("alias/test")),
            ],
        );
        let (provider, http) = provider([(200, "{}".into()), sqs_missing_queue_response()]);
        let _read = provider
            .update_sqs_queue(
                from.id.clone(),
                &AppliedIdentifier::new("https://example.test/123/removal.fifo").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("sqs.Queue"),
            )
            .await
            .expect("removal mutation");
        let requests = http.requests();
        assert!(
            requests[0].contains("action=AmazonSQS.SetQueueAttributes;"),
            "{}",
            requests[0]
        );
        let body: serde_json::Value =
            serde_json::from_str(requests[0].split_once("; body=").unwrap().1).unwrap();
        assert_eq!(body["Attributes"][wire_key], reset, "{key}: {body}");
        assert_eq!(body["QueueUrl"], "https://example.test/123/removal.fifo");
        http.assert_finished();
    }
}

#[tokio::test]
async fn queue_kms_reuse_removal_while_staying_sse_kms_sends_default_300() {
    const QUEUE_URL: &str = "https://example.test/123/removal";
    let from = snapshot(
        "sqs.Queue",
        [
            ("kms_master_key_id", string("alias/test")),
            ("kms_data_key_reuse_period_seconds", integer(60)),
            ("sqs_managed_sse_enabled", boolean(false)),
        ],
    );
    let (provider, http) = provider([(200, "{}".into()), sqs_missing_queue_response()]);
    let _read = provider
        .update_sqs_queue(
            from.id.clone(),
            &AppliedIdentifier::new(QUEUE_URL).unwrap(),
            &from,
            &remove(&from, "kms_data_key_reuse_period_seconds"),
            crate::test_resource_schema("sqs.Queue"),
        )
        .await
        .expect("SSE-KMS reuse-period removal must send the documented default");
    let requests = http.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].contains("action=AmazonSQS.SetQueueAttributes;"),
        "{}",
        requests[0]
    );
    let body: serde_json::Value =
        serde_json::from_str(requests[0].split_once("; body=").unwrap().1).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"QueueUrl": QUEUE_URL, "Attributes": {"KmsDataKeyReusePeriodSeconds": "300"}})
    );
    assert!(
        requests[1].contains("action=AmazonSQS.GetQueueAttributes;"),
        "{}",
        requests[1]
    );
    http.assert_finished();
}

#[tokio::test]
async fn queue_encryption_switches_send_only_the_resulting_mode() {
    const QUEUE_URL: &str = "https://example.test/123/removal";
    for (old, ops, expected_attributes) in [
        (
            vec![
                ("kms_master_key_id", string("alias/test")),
                ("kms_data_key_reuse_period_seconds", integer(60)),
                ("sqs_managed_sse_enabled", boolean(false)),
            ],
            vec![
                PatchOp::Remove {
                    key: "kms_master_key_id".into(),
                },
                PatchOp::Remove {
                    key: "kms_data_key_reuse_period_seconds".into(),
                },
                PatchOp::Replace {
                    key: "sqs_managed_sse_enabled".into(),
                    value: boolean(true),
                },
            ],
            serde_json::json!({"SqsManagedSseEnabled": "true"}),
        ),
        (
            vec![("sqs_managed_sse_enabled", boolean(true))],
            vec![
                PatchOp::Remove {
                    key: "sqs_managed_sse_enabled".into(),
                },
                PatchOp::Add {
                    key: "kms_master_key_id".into(),
                    value: string("alias/test"),
                },
                PatchOp::Add {
                    key: "kms_data_key_reuse_period_seconds".into(),
                    value: integer(60),
                },
            ],
            serde_json::json!({"KmsMasterKeyId": "alias/test", "KmsDataKeyReusePeriodSeconds": "60"}),
        ),
    ] {
        let from = snapshot(
            "sqs.Queue",
            [("queue_name", string("removal"))].into_iter().chain(old),
        );
        let to = UpdateResource::new(&from, &UpdatePatch { ops });
        let (provider, http) = provider([(200, "{}".into()), sqs_missing_queue_response()]);
        let _read = provider
            .update_sqs_queue(
                from.id.clone(),
                &AppliedIdentifier::new(QUEUE_URL).unwrap(),
                &from,
                &to,
                crate::test_resource_schema("sqs.Queue"),
            )
            .await
            .expect("a mode switch covers removal of the previous encryption settings");
        let requests = http.requests();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].contains("action=AmazonSQS.SetQueueAttributes;"),
            "{}",
            requests[0]
        );
        let body: serde_json::Value =
            serde_json::from_str(requests[0].split_once("; body=").unwrap().1).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"QueueUrl": QUEUE_URL, "Attributes": expected_attributes})
        );
        assert!(
            requests[1].contains("action=AmazonSQS.GetQueueAttributes;"),
            "{}",
            requests[1]
        );
        http.assert_finished();
    }
}

#[tokio::test]
async fn queue_kms_key_removal_without_a_mode_switch_is_rejected() {
    for remove_reuse_period in [false, true] {
        let from = snapshot(
            "sqs.Queue",
            [
                ("kms_master_key_id", string("alias/test")),
                ("kms_data_key_reuse_period_seconds", integer(60)),
                ("sqs_managed_sse_enabled", boolean(false)),
            ],
        );
        let mut ops = vec![PatchOp::Remove {
            key: "kms_master_key_id".into(),
        }];
        if remove_reuse_period {
            ops.push(PatchOp::Remove {
                key: "kms_data_key_reuse_period_seconds".into(),
            });
        }
        let to = UpdateResource::new(&from, &UpdatePatch { ops });
        let (provider, http) = provider([]);
        let error = provider
            .update_sqs_queue(
                from.id.clone(),
                &AppliedIdentifier::new("https://example.test/123/removal").unwrap(),
                &from,
                &to,
                crate::test_resource_schema("sqs.Queue"),
            )
            .await
            .expect_err("removing the key without selecting another mode has no documented reset");
        assert_removal_rejected(error, "kms_master_key_id");
        assert!(http.requests().is_empty());
        http.assert_finished();
    }
}

#[tokio::test]
async fn queue_kms_reuse_removal_outside_sse_kms_sends_no_attribute_request() {
    for sse_sqs in [None, Some(true)] {
        let from = snapshot(
            "sqs.Queue",
            [("kms_data_key_reuse_period_seconds", integer(60))]
                .into_iter()
                .chain(sse_sqs.map(|enabled| ("sqs_managed_sse_enabled", boolean(enabled)))),
        );
        let (provider, http) = provider([sqs_missing_queue_response()]);
        let _read = provider
            .update_sqs_queue(
                from.id.clone(),
                &AppliedIdentifier::new("https://example.test/123/removal").unwrap(),
                &from,
                &remove(&from, "kms_data_key_reuse_period_seconds"),
                crate::test_resource_schema("sqs.Queue"),
            )
            .await
            .expect("the reuse period is inapplicable outside SSE-KMS");
        let requests = http.requests();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].contains("action=AmazonSQS.GetQueueAttributes;"),
            "{}",
            requests[0]
        );
        http.assert_finished();
    }
}

#[tokio::test]
async fn route_target_can_be_replaced_in_the_same_patch() {
    for (removed, added) in [
        ("gateway_id", "nat_gateway_id"),
        ("nat_gateway_id", "gateway_id"),
    ] {
        let from = snapshot("ec2.Route", [(removed, string("old-target"))]);
        let to = UpdateResource::new(
            &from,
            &UpdatePatch {
                ops: vec![
                    PatchOp::Remove {
                        key: removed.into(),
                    },
                    PatchOp::Add {
                        key: added.into(),
                        value: string("new-target"),
                    },
                ],
            },
        );
        let (provider, http) = provider([
            (200, ec2_response("ReplaceRoute")),
            (200, ec2_response("DescribeRouteTables")),
        ]);
        provider
            .update_ec2_route(
                from.id.clone(),
                &AppliedIdentifier::new("rtb-test|0.0.0.0/0").unwrap(),
                &to,
            )
            .await
            .expect("replacement keeps a target")
            .expect("scripted read");
        let requests = http.requests();
        assert!(
            requests[0].contains("action=ReplaceRoute;"),
            "{}",
            requests[0]
        );
        let target_parameter = if added == "gateway_id" {
            "GatewayId=new-target"
        } else {
            "NatGatewayId=new-target"
        };
        assert!(requests[0].contains(target_parameter), "{}", requests[0]);
        assert!(!requests[0].contains("old-target"), "{}", requests[0]);
        http.assert_finished();
    }
}

#[tokio::test]
async fn route_without_target_removal_keeps_identifier_error() {
    let from = snapshot("ec2.Route", []);
    let to = UpdateResource::new(&from, &UpdatePatch { ops: vec![] });
    let (provider, http) = provider([]);
    let error = provider
        .update_ec2_route(
            from.id.clone(),
            &AppliedIdentifier::new("invalid").unwrap(),
            &to,
        )
        .await
        .expect_err("the identifier must be checked without inventing a removal");
    assert!(matches!(error, ProviderError::InvalidInput(_)));
    let error = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
    assert!(
        error.to_string().contains("Invalid route identifier"),
        "{error}"
    );
    assert!(
        !error
            .detail()
            .cause()
            .is_some_and(|cause| cause.is::<crate::helpers::AttributeRemovalError>())
    );
    assert!(http.requests().is_empty());
    http.assert_finished();
}

#[tokio::test]
async fn route_without_target_removal_keeps_api_error() {
    let from = snapshot("ec2.Route", []);
    let to = UpdateResource::new(&from, &UpdatePatch { ops: vec![] });
    let (provider, http) = provider([(
        400,
        "<Response><Errors><Error><Code>MissingParameter</Code><Message>A route target is required</Message></Error></Errors><RequestID>test</RequestID></Response>".into(),
    )]);
    let error = provider
        .update_ec2_route(
            from.id.clone(),
            &AppliedIdentifier::new("rtb-test|0.0.0.0/0").unwrap(),
            &to,
        )
        .await
        .expect_err("preserve the service error for a targetless input without a removal");
    assert!(matches!(error, ProviderError::ApiError(_)));
    let error = carina_core::test_utils::provider_text::BindProviderText::into_host_for_test(error);
    assert!(
        error.to_string().contains("Failed to update route"),
        "{error}"
    );
    assert!(http.requests()[0].contains("action=ReplaceRoute;"));
    http.assert_finished();
}

#[test]
fn generated_update_parts_reject_skipping_the_removal_decision() {
    for discard_with_pattern in [false, true] {
        let panic = std::panic::catch_unwind(|| {
            let from = snapshot(
                "ec2.InternetGateway",
                [("tags", map([("Name", string("test"))]))],
            );
            let to = remove(&from, "tags");
            let input =
                crate::send_models_generated::Ec2InternetGatewayUpdateInput::decode(&to).unwrap();
            if discard_with_pattern {
                let crate::send_models_generated::Ec2InternetGatewayUpdateParts { tags: _ } =
                    input.into_parts();
            } else {
                let crate::send_models_generated::Ec2InternetGatewayUpdateParts { tags } =
                    input.into_parts();
                let _ = tags;
            }
        })
        .expect_err("discarded generated update fields must trip the Sent guard");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap();
        assert!(
            message.contains("ec2.InternetGateway.update.tags"),
            "{message}"
        );
    }
}

#[tokio::test]
async fn route_sole_target_removal_is_rejected_without_requests() {
    for key in ["gateway_id", "nat_gateway_id"] {
        let from = snapshot("ec2.Route", [(key, string("target-test"))]);
        let (provider, http) = provider([]);
        let error = provider
            .update_ec2_route(
                from.id.clone(),
                &AppliedIdentifier::new("rtb-test|0.0.0.0/0").unwrap(),
                &remove(&from, key),
            )
            .await
            .expect_err("a route must retain a target");
        assert_removal_rejected(error, key);
        assert!(http.requests().is_empty());
        http.assert_finished();
    }
}

#[tokio::test]
async fn queue_undocumented_or_inapplicable_removals_are_rejected_without_requests() {
    for (key, old, extra) in [
        (
            "redrive_allow_policy",
            map([("redrive_permission", string("denyAll"))]),
            vec![],
        ),
        ("kms_master_key_id", string("alias/test"), vec![]),
        ("sqs_managed_sse_enabled", boolean(false), vec![]),
        (
            "policy",
            string(r#"{"Version":"2012-10-17","Statement":[]}"#),
            vec![],
        ),
        (
            "redrive_policy",
            map([
                (
                    "dead_letter_target_arn",
                    string("arn:aws:sqs:us-east-1:123:dead"),
                ),
                ("max_receive_count", integer(3)),
            ]),
            vec![],
        ),
        (
            "content_based_deduplication",
            boolean(false),
            vec![("fifo_queue", boolean(false))],
        ),
    ] {
        let from = snapshot("sqs.Queue", [(key, old)].into_iter().chain(extra));
        let (provider, http) = provider([]);
        let error = provider
            .update_sqs_queue(
                from.id.clone(),
                &AppliedIdentifier::new("https://example.test/123/removal").unwrap(),
                &from,
                &remove(&from, key),
                crate::test_resource_schema("sqs.Queue"),
            )
            .await
            .expect_err("unsupported removal must fail before mutations");
        assert_removal_rejected(error, key);
        assert!(http.requests().is_empty());
        http.assert_finished();
    }
}
