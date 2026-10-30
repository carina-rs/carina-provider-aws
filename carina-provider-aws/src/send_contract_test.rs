//! Transport-level smoke coverage for the generated managed-resource send
//! contract. Mutually exclusive schema inputs are exercised in separate,
//! minimal create variants; the generated input type is the primary
//! exhaustive guarantee.

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::str;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use carina_core::provider::{
    AppliedIdentifier, Observation, PatchOp, ProviderError, Sent, UpdatePatch,
};
use carina_core::resource::{ConcreteValue, Resource, State, Value};
use carina_core::schema::{
    AttributeType, ResourceSchema, SchemaKind, Shape, ShapeWalkBudget, ValidatedResourceSchema,
};
use indexmap::IndexMap;
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

use crate::provider::{AwsCreateAttempt, AwsCreateToken, AwsIssuedCreate};
use crate::send_models_generated::{SendCaptureAttribute, SendCaptureContract};
use crate::{AwsProvider, BoundedSdkConfig};

const REGION: &str = "us-east-1";

#[test]
fn every_managed_resource_has_a_generated_capture_contract() {
    let managed = crate::schemas::all_schemas()
        .into_iter()
        .filter(|schema| schema.as_schema().kind() == SchemaKind::Resource)
        .map(|schema| schema.as_schema().resource_type().to_string())
        .collect::<BTreeSet<_>>();
    let covered = crate::send_models_generated::capture_contracts()
        .iter()
        .map(|contract| contract.resource_type.to_string())
        .collect::<BTreeSet<_>>();

    assert_eq!(covered, managed);
}

#[derive(Default)]
struct CaptureService {
    requests: Mutex<Vec<MockRequest>>,
}

impl CaptureService {
    fn clear(&self) {
        self.requests.lock().expect("capture lock").clear();
    }

    fn snapshot(&self) -> Vec<MockRequest> {
        self.requests.lock().expect("capture lock").clone()
    }

    fn response_for(request: &MockRequest) -> MockResponse {
        let body = str::from_utf8(&request.body).unwrap_or_default();
        let decoded_body = urlencoding::decode(body)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| body.to_string());
        let target = request
            .headers
            .get("x-amz-target")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let action = decoded_body
            .split('&')
            .find_map(|parameter| parameter.strip_prefix("Action="))
            .unwrap_or_default();

        if request.uri.contains("ec2.") {
            let inner = match action {
                "CreateVpc" => "<vpc><vpcId>vpc-0123456789abcdef0</vpcId></vpc>",
                "CreateSubnet" => "<subnet><subnetId>subnet-0123456789abcdef0</subnetId></subnet>",
                "CreateInternetGateway" => {
                    "<internetGateway><internetGatewayId>igw-0123456789abcdef0</internetGatewayId></internetGateway>"
                }
                _ => "",
            };
            return MockResponse::xml(
                200,
                format!(
                    "<{action}Response xmlns=\"http://ec2.amazonaws.com/doc/2016-11-15/\"><requestId>send-contract</requestId>{inner}</{action}Response>"
                ),
            );
        }
        if request.uri.contains("route53") {
            return MockResponse::xml(
                200,
                r#"<ChangeResourceRecordSetsResponse xmlns="https://route53.amazonaws.com/doc/2013-04-01/"><ChangeInfo><Id>/change/C1</Id><Status>PENDING</Status><SubmittedAt>2026-01-01T00:00:00Z</SubmittedAt></ChangeInfo></ChangeResourceRecordSetsResponse>"#,
            );
        }
        if request.uri.contains("iam.") {
            return MockResponse::xml(
                200,
                format!(
                    "<{action}Response xmlns=\"https://iam.amazonaws.com/doc/2010-05-08/\"><{action}Result/></{action}Response>"
                ),
            );
        }
        if request.uri.contains("s3.") || request.uri.contains(".s3.") {
            if request.method == "GET" && request.uri.contains("?versioning") {
                return MockResponse::xml(
                    200,
                    r#"<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Status>Enabled</Status></VersioningConfiguration>"#,
                );
            }
            return MockResponse::xml(200, "");
        }
        if target.ends_with("RequestCertificate") {
            return MockResponse::json(
                200,
                r#"{"CertificateArn":"arn:aws:acm:us-east-1:123456789012:certificate/00000000-0000-0000-0000-000000000000"}"#,
            );
        }
        if target.ends_with("CreateOrganization") || target.ends_with("DescribeOrganization") {
            return MockResponse::json(
                200,
                r#"{"Organization":{"Id":"o-exampleorgid","Arn":"arn:aws:organizations::123456789012:organization/o-exampleorgid","FeatureSet":"ALL","MasterAccountArn":"arn:aws:organizations::123456789012:account/o-exampleorgid/123456789012","MasterAccountEmail":"root@example.com","MasterAccountId":"123456789012"}}"#,
            );
        }
        if target.ends_with("CreateAccount") {
            return MockResponse::json(
                200,
                r#"{"CreateAccountStatus":{"Id":"car-create-request","State":"IN_PROGRESS"}}"#,
            );
        }
        if target.ends_with("CreateQueue") {
            return MockResponse::json(
                200,
                r#"{"QueueUrl":"https://sqs.us-east-1.amazonaws.com/123456789012/carina-send-contract.fifo"}"#,
            );
        }
        MockResponse::json(200, "{}")
    }
}

impl MockService for CaptureService {
    fn service_name(&self) -> &str {
        "capture"
    }

    fn url_patterns(&self) -> Vec<&str> {
        vec![r"^https://"]
    }

    fn handle(
        &self,
        request: MockRequest,
    ) -> Pin<Box<dyn Future<Output = MockResponse> + Send + '_>> {
        let response = Self::response_for(&request);
        self.requests.lock().expect("capture lock").push(request);
        Box::pin(async move { response })
    }
}

struct CaptureAttempt;

struct CaptureIssued {
    identifier: AppliedIdentifier,
}

impl AwsCreateAttempt for CaptureAttempt {
    type Step = ();
    type Verdict = ProviderError;
    type Issued = CaptureIssued;

    fn untracked(self, error: ProviderError) -> Self::Verdict {
        error
    }

    fn issued(self, identifier: AppliedIdentifier, _sent: Sent) -> Self::Issued {
        CaptureIssued { identifier }
    }

    fn pending(
        self,
        _token: AwsCreateToken,
        _sent: Sent,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(())
    }
}

impl AwsIssuedCreate for CaptureIssued {
    type Step = ();
    type Verdict = ProviderError;

    fn identifier(&self) -> &AppliedIdentifier {
        &self.identifier
    }

    fn succeeded(self, _attributes: HashMap<String, Value>) -> Result<Self::Step, Self::Verdict> {
        Ok(())
    }

    fn partial(
        self,
        _attributes: HashMap<String, Value>,
        _reason: String,
        _missing_attributes: Vec<String>,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(())
    }

    fn failed(self, _observation: Observation, error: ProviderError) -> Self::Verdict {
        error
    }

    fn discarded(self, error: ProviderError) -> Self::Verdict {
        error
    }

    fn pending(
        self,
        _token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(())
    }
}

fn string(value: impl Into<String>) -> Value {
    Value::Concrete(ConcreteValue::String(value.into().into()))
}

fn sample_string(resource_type: &str, path: &[String]) -> String {
    let name = path.last().map(String::as_str).unwrap_or_default();
    let hash = resource_type
        .bytes()
        .chain(path.iter().flat_map(|part| part.bytes()))
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            hash.wrapping_mul(0x100000001b3) ^ u64::from(byte)
        });
    let suffix = format!("{:08x}", hash as u32);
    let ec2_id = |prefix: &str| format!("{prefix}-0{hash:016x}");
    match name {
        "account_name" => format!("send-contract-{suffix}"),
        "address" => format!("198.51.100.{}", 10 + hash % 200),
        "allocation_id" | "allocation_ids" => ec2_id("eipalloc"),
        "arn"
        | "deliver_logs_permission_arn"
        | "kms_key_id"
        | "log_destination"
        | "resource_configuration_arn"
        | "role"
        | "service_network_arn" => {
            format!("arn:aws:iam::123456789012:role/send-contract-{suffix}")
        }
        "availability_zone" => "us-east-1a".to_string(),
        "availability_zone_id" => "use1-az1".to_string(),
        "bucket" | "target_bucket" => format!("carina-send-{suffix}"),
        "cidr_block" | "cidr_ip" | "destination_cidr_block" => {
            format!("10.{}.{}.0/24", 1 + (hash >> 8) % 200, 1 + hash % 200)
        }
        "cidr_ipv6" | "ipv6_cidr_block" => {
            format!("2001:db8:{:x}::/64", hash as u16)
        }
        "description" => format!("send contract {suffix}"),
        "dns_name" | "domain_name" | "host_name" | "name" => {
            format!("send-{suffix}.example.com")
        }
        "email" => format!("send-{suffix}@example.com"),
        "gateway_id" | "internet_gateway_id" => ec2_id("igw"),
        "group_id" | "destination_security_group_id" | "source_security_group_id" => ec2_id("sg"),
        "group_name" => format!("send-contract-{suffix}"),
        "hosted_zone_id" => format!("Z{hash:016X}"),
        "ipv4_ipam_pool_id" | "ipv6_ipam_pool_id" => ec2_id("ipam-pool"),
        "log_group_name" => format!("/carina/send-{suffix}"),
        "nat_gateway_id" => ec2_id("nat"),
        "outpost_arn" => {
            "arn:aws:outposts:us-east-1:123456789012:outpost/op-0123456789abcdef0".to_string()
        }
        "path" => "/".to_string(),
        "peer_owner_id" | "source_security_group_owner_id" => "123456789012".to_string(),
        "peer_vpc_id" | "vpc_id" => ec2_id("vpc"),
        "private_ip_address" => format!("10.0.0.{}", 10 + hash % 200),
        "public_ipv4_pool" => "amazon".to_string(),
        "queue_name" => format!("carina-send-{suffix}.fifo"),
        "resource_records" => "192.0.2.10".to_string(),
        "role_name" => format!("CarinaSend{suffix}"),
        "route_table_id" | "route_table_ids" => ec2_id("rtb"),
        "service_name" => "com.amazonaws.us-east-1.s3".to_string(),
        "service_region" => REGION.to_string(),
        "subnet_id" | "subnet_ids" => ec2_id("subnet"),
        "suffix" => "index.html".to_string(),
        "target_prefix" => "logs/".to_string(),
        "transit_gateway_id" => ec2_id("tgw"),
        "value" if resource_type == "route53.RecordSet" => "192.0.2.10".to_string(),
        "vpn_gateway_id" => ec2_id("vgw"),
        _ => format!("send-contract-{suffix}"),
    }
}

fn sample_value(
    resource_type: &str,
    schema: &ResourceSchema,
    attribute_type: &AttributeType,
    path: &mut Vec<String>,
    budget: &mut ShapeWalkBudget,
) -> Value {
    match schema.shape_of(attribute_type) {
        Shape::String { .. } => string(sample_string(resource_type, path)),
        Shape::Int { range, .. } => {
            let minimum = range.and_then(|(minimum, _)| minimum).unwrap_or(1);
            let maximum = range.and_then(|(_, maximum)| maximum).unwrap_or(i64::MAX);
            let preferred = match path.last().map(String::as_str) {
                Some("from_port" | "to_port") => 443,
                Some("max_session_duration") => 3600,
                Some("retention_in_days") => 1,
                Some("ttl") => 300,
                _ => 1,
            };
            Value::Concrete(ConcreteValue::Int(preferred.clamp(minimum, maximum)))
        }
        Shape::Float { range, .. } => {
            let minimum = range.and_then(|(minimum, _)| minimum).unwrap_or(1.0);
            let maximum = range.and_then(|(_, maximum)| maximum).unwrap_or(f64::MAX);
            Value::Concrete(ConcreteValue::Float(1.0_f64.clamp(minimum, maximum)))
        }
        Shape::Bool => Value::Concrete(ConcreteValue::Bool(
            path.last()
                .is_none_or(|name| name != "evaluate_target_health"),
        )),
        Shape::Duration => Value::Concrete(ConcreteValue::Duration(Duration::from_secs(60))),
        Shape::Enum { values, base, .. } => values.and_then(|values| values.first()).map_or_else(
            || sample_value(resource_type, schema, base, path, budget),
            |value| string(value.clone()),
        ),
        Shape::List { element_type, .. } => {
            Value::Concrete(ConcreteValue::List(vec![sample_value(
                resource_type,
                schema,
                element_type,
                path,
                budget,
            )]))
        }
        Shape::Map { value, .. } => {
            let key = if path.last().is_some_and(|name| name == "tags") {
                "Name"
            } else {
                "key"
            };
            Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                key.to_string(),
                sample_value(resource_type, schema, value, path, budget),
            )])))
        }
        Shape::Struct { .. } => {
            let fields = schema
                .struct_fields_with_budget(attribute_type, budget)
                .expect("send-contract struct walk must stay within its budget");
            let mut map = IndexMap::new();
            let has_required = fields
                .iter()
                .any(|field| field.parts().declared_input_mode.is_required());
            let mut fallback = None;
            for field in fields
                .iter()
                .filter(|field| !field.parts().declared_input_mode.is_provider_populated())
            {
                let parts = field.parts();
                if has_required && !parts.declared_input_mode.is_required() {
                    continue;
                }
                path.push(parts.name.to_string());
                let sample = sample_value(resource_type, schema, parts.field_type, path, budget);
                path.pop();
                if has_required {
                    map.insert(parts.name.to_string(), sample);
                } else if value_has_scalar(&sample) {
                    map.insert(parts.name.to_string(), sample);
                    break;
                } else if fallback.is_none() {
                    fallback = Some((parts.name.to_string(), sample));
                }
            }
            if map.is_empty()
                && let Some((name, sample)) = fallback
            {
                map.insert(name, sample);
            }
            Value::Concrete(ConcreteValue::Map(map))
        }
        Shape::Union => {
            let members = schema
                .union_members_with_budget(attribute_type, budget)
                .expect("send-contract union walk must stay within its budget");
            let samples = members
                .iter()
                .flatten()
                .map(|member| sample_value(resource_type, schema, member.as_attr(), path, budget))
                .collect::<Vec<_>>();
            samples
                .iter()
                .find(|sample| value_has_scalar(sample))
                .cloned()
                .or_else(|| samples.into_iter().next())
                .expect("generated send-contract union must have a resolvable member")
        }
    }
}

fn value_has_scalar(value: &Value) -> bool {
    match value {
        Value::Concrete(
            ConcreteValue::String(_)
            | ConcreteValue::StringList(_)
            | ConcreteValue::CanonicalEnum(_)
            | ConcreteValue::EnumIdentifier(_)
            | ConcreteValue::Int(_)
            | ConcreteValue::Float(_)
            | ConcreteValue::Bool(_)
            | ConcreteValue::Duration(_),
        ) => true,
        Value::Concrete(ConcreteValue::List(values)) => values.iter().any(value_has_scalar),
        Value::Concrete(ConcreteValue::Map(values)) => values.values().any(value_has_scalar),
        Value::Deferred(_) => false,
    }
}

fn set_sample(resource: &mut Resource, schema: &ValidatedResourceSchema, name: &str) {
    let attribute = schema.effective_attribute(name).unwrap_or_else(|| {
        panic!(
            "{} has no schema attribute {name}",
            resource.id.resource_type
        )
    });
    let mut path = vec![name.to_string()];
    let value = sample_value(
        &resource.id.resource_type,
        schema.as_schema(),
        attribute.field_type(),
        &mut path,
        &mut ShapeWalkBudget::new(64),
    );
    resource.set_attr(name, attribute.canonicalize_state(value));
}

fn set_bool(resource: &mut Resource, name: &str, value: bool) {
    resource.set_attr(name, Value::Concrete(ConcreteValue::Bool(value)));
}

fn set_canonical_string(
    resource: &mut Resource,
    schema: &ValidatedResourceSchema,
    name: &str,
    value: &str,
) {
    let attribute = schema.effective_attribute(name).unwrap_or_else(|| {
        panic!(
            "{} has no schema attribute {name}",
            resource.id.resource_type
        )
    });
    resource.set_attr(name, attribute.canonicalize_state(string(value)));
}

fn focused_resource(
    schema: &ValidatedResourceSchema,
    contract: &SendCaptureContract,
    target: &SendCaptureAttribute,
) -> Resource {
    let mut resource =
        Resource::with_provider("aws", contract.resource_type, "send-contract", None);
    for attribute in schema.effective_attributes() {
        if attribute.is_required() && !attribute.is_read_only() {
            set_sample(&mut resource, schema, attribute.name());
        }
    }
    set_sample(&mut resource, schema, target.name);

    match contract.resource_type {
        "ec2.Vpc" => {
            if matches!(target.name, "ipv4_ipam_pool_id" | "ipv4_netmask_length") {
                resource.attributes.shift_remove("cidr_block");
                set_sample(&mut resource, schema, "ipv4_ipam_pool_id");
                set_sample(&mut resource, schema, "ipv4_netmask_length");
            } else {
                set_sample(&mut resource, schema, "cidr_block");
            }
        }
        "ec2.Subnet" => {
            if target.name == "availability_zone" {
                resource.attributes.shift_remove("availability_zone_id");
            } else if target.name == "availability_zone_id" {
                resource.attributes.shift_remove("availability_zone");
            }
            if matches!(target.name, "ipv4_ipam_pool_id" | "ipv4_netmask_length") {
                resource.attributes.shift_remove("cidr_block");
                set_sample(&mut resource, schema, "ipv4_ipam_pool_id");
                set_sample(&mut resource, schema, "ipv4_netmask_length");
            } else {
                set_sample(&mut resource, schema, "cidr_block");
            }
            if matches!(target.name, "ipv6_ipam_pool_id" | "ipv6_netmask_length") {
                set_sample(&mut resource, schema, "ipv6_ipam_pool_id");
                set_sample(&mut resource, schema, "ipv6_netmask_length");
            }
            if target.name == "ipv6_native" {
                resource.attributes.shift_remove("cidr_block");
                set_sample(&mut resource, schema, "ipv6_cidr_block");
                set_bool(&mut resource, "ipv6_native", true);
            }
        }
        "ec2.Route" => {
            set_sample(&mut resource, schema, "destination_cidr_block");
            if !matches!(target.name, "gateway_id" | "nat_gateway_id") {
                set_sample(&mut resource, schema, "gateway_id");
            }
        }
        "ec2.SecurityGroupIngress" | "ec2.SecurityGroupEgress" => {
            set_sample(&mut resource, schema, "group_id");
            if target.name == "description" {
                set_sample(&mut resource, schema, "cidr_ip");
            }
            if target.name == "source_security_group_owner_id" {
                let referenced_group = if contract.resource_type == "ec2.SecurityGroupIngress" {
                    "source_security_group_id"
                } else {
                    "destination_security_group_id"
                };
                set_sample(&mut resource, schema, referenced_group);
            }
        }
        "ec2.NatGateway" => {
            if target.name == "availability_mode" {
                set_canonical_string(&mut resource, schema, "availability_mode", "zonal");
            }
            set_sample(&mut resource, schema, "subnet_id");
        }
        "ec2.VpcGatewayAttachment" => {
            if target.name == "vpn_gateway_id" {
                resource.attributes.shift_remove("internet_gateway_id");
            } else if target.name != "internet_gateway_id" {
                set_sample(&mut resource, schema, "internet_gateway_id");
            }
        }
        "route53.RecordSet" => {
            set_sample(&mut resource, schema, "hosted_zone_id");
            if target.name == "alias_target" {
                resource.attributes.shift_remove("resource_records");
                resource.attributes.shift_remove("ttl");
            } else {
                set_sample(&mut resource, schema, "resource_records");
                set_sample(&mut resource, schema, "ttl");
            }
        }
        "sqs.Queue" => {
            set_bool(&mut resource, "fifo_queue", true);
            resource.set_attr("queue_name", string("carina-send-contract.fifo"));
            if matches!(
                target.name,
                "content_based_deduplication" | "sqs_managed_sse_enabled"
            ) {
                set_bool(&mut resource, target.name, false);
            }
            if target.name == "kms_data_key_reuse_period_seconds" {
                set_sample(&mut resource, schema, "kms_master_key_id");
            }
        }
        "acm.Certificate" => {
            set_canonical_string(&mut resource, schema, "validation_method", "EMAIL");
        }
        _ => {}
    }
    resource
}

fn decoded_request_body(request: &MockRequest) -> String {
    let body = str::from_utf8(&request.body).unwrap_or_default();
    urlencoding::decode(body)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| body.to_string())
}

fn request_operation(request: &MockRequest) -> String {
    let target = request
        .headers
        .get("x-amz-target")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit('.').next())
        .unwrap_or_default();
    if !target.is_empty() {
        return target.to_string();
    }
    decoded_request_body(request)
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("Action="))
        .unwrap_or_default()
        .to_string()
}

fn is_mutating_request(request: &MockRequest) -> bool {
    if matches!(request.method.as_str(), "GET" | "HEAD") {
        return false;
    }
    let operation = request_operation(request);
    !["Describe", "Get", "List", "Head", "Lookup"]
        .iter()
        .any(|prefix| operation.starts_with(prefix))
}

fn mutating_request_payload(requests: &[MockRequest]) -> String {
    requests
        .iter()
        .filter(|request| is_mutating_request(request))
        .map(|request| format!("{}\n{}", request.uri, decoded_request_body(request)))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase()
}

fn sample_markers(value: &Value, markers: &mut Vec<String>) {
    match value {
        Value::Concrete(ConcreteValue::String(value)) => {
            markers.push(value.expose_plaintext().to_string())
        }
        Value::Concrete(ConcreteValue::CanonicalEnum(value)) => {
            markers.push(value.api_value().to_string())
        }
        Value::Concrete(ConcreteValue::Int(value)) => markers.push(value.to_string()),
        Value::Concrete(ConcreteValue::Float(value)) => markers.push(value.to_string()),
        Value::Concrete(ConcreteValue::Bool(value)) => markers.push(value.to_string()),
        Value::Concrete(ConcreteValue::Duration(value)) => {
            markers.push(value.as_secs().to_string())
        }
        Value::Concrete(ConcreteValue::List(values)) => {
            for value in values {
                sample_markers(value, markers);
            }
        }
        Value::Concrete(ConcreteValue::StringList(values)) => {
            markers.extend(
                values
                    .iter()
                    .map(|value| value.expose_plaintext().to_string()),
            );
        }
        Value::Concrete(ConcreteValue::Map(values)) => {
            for value in values.values() {
                sample_markers(value, markers);
            }
        }
        Value::Concrete(ConcreteValue::EnumIdentifier(value)) => {
            markers.push(format!("{value:?}"));
        }
        Value::Deferred(_) => panic!("send-contract samples must be concrete"),
    }
}

fn payload_contains_marker(payload: &str, marker: &str) -> bool {
    let marker = marker.to_ascii_lowercase();
    if marker.len() > 3 && payload.contains(&marker) {
        return true;
    }
    if payload.ends_with(&format!("={marker}")) {
        return true;
    }
    [
        format!("={marker}&"),
        format!("={marker}\n"),
        format!("={marker}<"),
        format!(">{marker}<"),
        format!(":{marker},"),
        format!(":{marker}}}"),
        format!(":\"{marker}\""),
        format!("/{marker}/"),
    ]
    .iter()
    .any(|needle| payload.contains(needle))
}

fn capture_identifier(resource_type: &str) -> &'static str {
    match resource_type {
        "acm.Certificate" => {
            "arn:aws:acm:us-east-1:123456789012:certificate/00000000-0000-0000-0000-000000000000"
        }
        "iam.Role" => "CarinaSendContract",
        "logs.LogGroup" => "/carina/send-contract",
        "organizations.Account" => "123456789012",
        "route53.RecordSet" => "Z0123456789ABCDEF|send-contract.example.com|A",
        "s3.Bucket"
        | "s3.BucketAcl"
        | "s3.BucketCorsConfiguration"
        | "s3.BucketLifecycleConfiguration"
        | "s3.BucketLogging"
        | "s3.BucketNotificationConfiguration"
        | "s3.BucketObjectLockConfiguration"
        | "s3.BucketOwnershipControls"
        | "s3.BucketPolicy"
        | "s3.BucketPublicAccessBlock"
        | "s3.BucketReplicationConfiguration"
        | "s3.BucketServerSideEncryptionConfiguration"
        | "s3.BucketVersioning"
        | "s3.BucketWebsiteConfiguration" => "carina-send-contract-bucket",
        "sqs.Queue" => "https://sqs.us-east-1.amazonaws.com/123456789012/carina-send-contract.fifo",
        "ec2.Vpc" => "vpc-0123456789abcdef0",
        "ec2.Subnet" => "subnet-0123456789abcdef0",
        "ec2.InternetGateway" => "igw-0123456789abcdef0",
        "ec2.RouteTable" => "rtb-0123456789abcdef0",
        "ec2.Route" => "rtb-0123456789abcdef0|10.0.0.0/24",
        "ec2.SecurityGroup" => "sg-0123456789abcdef0",
        "ec2.Eip" => "eipalloc-0123456789abcdef0",
        "ec2.NatGateway" => "nat-0123456789abcdef0",
        "ec2.VpnGateway" => "vgw-0123456789abcdef0",
        "ec2.TransitGateway" => "tgw-0123456789abcdef0",
        "ec2.TransitGatewayAttachment" => "tgw-attach-0123456789abcdef0",
        "ec2.VpcEndpoint" => "vpce-0123456789abcdef0",
        "ec2.VpcPeeringConnection" => "pcx-0123456789abcdef0",
        "ec2.FlowLog" => "fl-0123456789abcdef0",
        "ec2.EgressOnlyInternetGateway" => "eigw-0123456789abcdef0",
        "ec2.VpcGatewayAttachment" => "vpc-0123456789abcdef0|igw-0123456789abcdef0",
        "ec2.SubnetRouteTableAssociation" => "rtbassoc-0123456789abcdef0|subnet-0123456789abcdef0",
        _ => "send-contract-identifier",
    }
}

fn existing_state_without(resource: &Resource, excluded: Option<&str>) -> State {
    crate::read_model::test_existing_state(
        resource.id.clone(),
        resource
            .attributes
            .iter()
            .filter(|(name, _)| excluded != Some(name.as_str()))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
    )
}

fn assert_sample_reached_mutation(
    contract: &SendCaptureContract,
    attribute: &SendCaptureAttribute,
    resource: &Resource,
    requests: &[MockRequest],
    operation: &str,
) {
    let value = resource.get_attr(attribute.name).unwrap_or_else(|| {
        panic!(
            "missing synthesized {}.{}",
            contract.resource_type, attribute.name
        )
    });
    let mut markers = Vec::new();
    sample_markers(value, &mut markers);
    let other_markers = resource
        .attributes
        .iter()
        .filter(|(name, _)| name.as_str() != attribute.name)
        .flat_map(|(_, value)| {
            let mut markers = Vec::new();
            sample_markers(value, &mut markers);
            markers
        })
        .map(|marker| marker.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let markers = markers
        .into_iter()
        .filter(|marker| !other_markers.contains(&marker.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    let payload = mutating_request_payload(requests);
    // Smithy models EventBridgeConfiguration as an empty structure: its
    // presence is the entire value, so there is no scalar sample to search
    // for. Require its explicitly declared structural token instead.
    let exact_presence_match = attribute.capture_names.iter().any(|mapping| {
        mapping.strip_prefix("presence:").is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            payload.contains(&format!("<{name}")) || payload.contains(&format!("%3c{name}"))
        })
    });
    assert!(
        !markers.is_empty() || exact_presence_match,
        "{}.{} synthesized no scalar request marker unique among desired attributes",
        contract.resource_type,
        attribute.name
    );
    let mappings = (attribute.provider_name, attribute.capture_names);
    // Do not put headers in the searchable haystack: generic headers such as
    // Content-Type caused the original false positives. The two Smithy
    // `@httpHeader` inputs have explicit `header:` mappings and are compared
    // by exact header name and exact synthesized value instead.
    let exact_header_match = attribute.capture_names.iter().any(|mapping| {
        let Some(header_name) = mapping.strip_prefix("header:") else {
            return false;
        };
        requests
            .iter()
            .filter(|request| is_mutating_request(request))
            .filter_map(|request| request.headers.get(header_name))
            .filter_map(|value| value.to_str().ok())
            .any(|value| markers.iter().any(|marker| value == marker))
    });
    assert!(
        exact_header_match
            || exact_presence_match
            || markers
                .iter()
                .any(|marker| payload_contains_marker(&payload, marker)),
        "{}.{} synthesized values {:?} were absent from mutating {operation} URI/query/body (mapping={mappings:?}); mutating payload={payload:?}; requests={requests:?}",
        contract.resource_type,
        attribute.name,
        markers,
    );
}

#[tokio::test]
async fn every_sent_attribute_reaches_captured_create_and_update_mutations() {
    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::new(mock.sdk_config(REGION).await),
    );
    let schemas = crate::schemas::all_schemas();

    for contract in crate::send_models_generated::capture_contracts() {
        let schema = schemas
            .iter()
            .find(|schema| {
                schema.as_schema().kind() == SchemaKind::Resource
                    && schema.as_schema().resource_type() == contract.resource_type
            })
            .unwrap_or_else(|| panic!("missing schema for {}", contract.resource_type));
        for attribute in contract.create_attributes {
            service.clear();
            let resource = focused_resource(schema, contract, attribute);
            let result = provider.create_step(&resource, CaptureAttempt).await;
            let requests = service.snapshot();
            assert!(
                !requests.is_empty(),
                "{}.{} failed before reaching the capture transport; result={result:?}; desired={:?}",
                contract.resource_type,
                attribute.name,
                resource.attributes
            );
            assert_sample_reached_mutation(contract, attribute, &resource, &requests, "create");
        }

        let update_targets: Vec<Option<&SendCaptureAttribute>> =
            if contract.update_attributes.is_empty() {
                vec![None]
            } else {
                contract.update_attributes.iter().map(Some).collect()
            };
        for target in update_targets {
            service.clear();
            let builder_attribute = target.unwrap_or_else(|| {
                contract
                    .create_attributes
                    .first()
                    .expect("managed create contract has at least one field")
            });
            let resource = focused_resource(schema, contract, builder_attribute);
            let from = existing_state_without(&resource, target.map(|attribute| attribute.name));
            let patch = UpdatePatch {
                ops: target
                    .map(|attribute| PatchOp::Add {
                        key: attribute.name.to_string(),
                        value: resource
                            .get_attr(attribute.name)
                            .expect("focused update value")
                            .clone(),
                    })
                    .into_iter()
                    .collect(),
            };
            let identifier = crate::helpers::applied_identifier(
                &resource.id,
                "send-contract.Update",
                capture_identifier(contract.resource_type),
            )
            .expect("capture identifier");
            let result = provider
                .update_with_patch(&resource.id, &identifier, &from, &patch)
                .await;
            let requests = service.snapshot();
            assert!(
                !requests.is_empty(),
                "{} update failed before reaching the capture transport; target={:?}; result={result:?}; desired={:?}",
                contract.resource_type,
                target.map(|attribute| attribute.name),
                resource.attributes,
            );
            if let Some(attribute) = target {
                assert_sample_reached_mutation(contract, attribute, &resource, &requests, "update");
            }
        }
    }
}
