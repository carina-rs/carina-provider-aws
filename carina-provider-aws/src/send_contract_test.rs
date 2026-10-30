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

use carina_core::provider::{AppliedIdentifier, Observation, ProviderError, Sent};
use carina_core::resource::{ConcreteValue, Resource, Value};
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

#[test]
fn send_input_consumers_do_not_use_wildcard_or_underscore_holes() {
    fn visit(path: &std::path::Path, sources: &mut Vec<std::path::PathBuf>) {
        if path.is_dir() {
            for entry in std::fs::read_dir(path).expect("read provider source directory") {
                visit(&entry.expect("read provider source entry").path(), sources);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path.to_path_buf());
        }
    }

    let mut sources = Vec::new();
    visit(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut sources,
    );
    const INTO_PARTS_CALL: &str = concat!("input.into_", "parts()");
    for path in sources {
        let source = std::fs::read_to_string(&path).expect("read provider source");
        for (call_offset, _) in source.match_indices(INTO_PARTS_CALL) {
            let prefix = &source[..call_offset];
            let let_offset = prefix.rfind("let ").unwrap_or_else(|| {
                panic!("{} has into_parts() without a let binding", path.display())
            });
            let binding = &prefix[let_offset + 4..];
            let tuple_pattern = binding
                .trim()
                .strip_suffix('=')
                .map(str::trim)
                .unwrap_or(binding);
            assert!(
                tuple_pattern.starts_with('(') && tuple_pattern.ends_with(')'),
                "{} stores into_parts() without exhaustive tuple destructuring in `{binding}`",
                path.display()
            );
            assert!(
                !binding.contains(".."),
                "{} uses a rest hole in `{binding}`",
                path.display()
            );
            let identifiers = binding
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .filter(|identifier| !identifier.is_empty());
            for identifier in identifiers {
                assert!(
                    !identifier.starts_with('_'),
                    "{} uses an underscore hole `{identifier}` in `{binding}`",
                    path.display()
                );
            }
        }
    }
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
        if target.ends_with("CreateOrganization") {
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
    match name {
        "account_name" => "send-contract-account".to_string(),
        "address" => "198.51.100.10".to_string(),
        "allocation_id" | "allocation_ids" => "eipalloc-0123456789abcdef0".to_string(),
        "arn"
        | "deliver_logs_permission_arn"
        | "kms_key_id"
        | "log_destination"
        | "resource_configuration_arn"
        | "role"
        | "service_network_arn" => "arn:aws:iam::123456789012:role/send-contract".to_string(),
        "availability_zone" => "us-east-1a".to_string(),
        "availability_zone_id" => "use1-az1".to_string(),
        "bucket" | "target_bucket" => "carina-send-contract-bucket".to_string(),
        "cidr_block" | "cidr_ip" | "destination_cidr_block" => "10.0.0.0/24".to_string(),
        "cidr_ipv6" | "ipv6_cidr_block" => "2001:db8::/64".to_string(),
        "description" => "send contract".to_string(),
        "dns_name" | "domain_name" | "host_name" | "name" => {
            "send-contract.example.com".to_string()
        }
        "email" => "send-contract@example.com".to_string(),
        "gateway_id" | "internet_gateway_id" => "igw-0123456789abcdef0".to_string(),
        "group_id" | "destination_security_group_id" | "source_security_group_id" => {
            "sg-0123456789abcdef0".to_string()
        }
        "group_name" => "send-contract-group".to_string(),
        "hosted_zone_id" => "Z0123456789ABCDEF".to_string(),
        "ipv4_ipam_pool_id" | "ipv6_ipam_pool_id" => "ipam-pool-0123456789abcdef0".to_string(),
        "log_group_name" => "/carina/send-contract".to_string(),
        "nat_gateway_id" => "nat-0123456789abcdef0".to_string(),
        "outpost_arn" => {
            "arn:aws:outposts:us-east-1:123456789012:outpost/op-0123456789abcdef0".to_string()
        }
        "path" => "/".to_string(),
        "peer_owner_id" | "source_security_group_owner_id" => "123456789012".to_string(),
        "peer_vpc_id" | "vpc_id" => "vpc-0123456789abcdef0".to_string(),
        "private_ip_address" => "10.0.0.10".to_string(),
        "public_ipv4_pool" => "amazon".to_string(),
        "queue_name" => "carina-send-contract.fifo".to_string(),
        "resource_records" => "192.0.2.10".to_string(),
        "role_name" => "CarinaSendContract".to_string(),
        "route_table_id" | "route_table_ids" => "rtb-0123456789abcdef0".to_string(),
        "service_name" => "com.amazonaws.us-east-1.s3".to_string(),
        "service_region" => REGION.to_string(),
        "subnet_id" | "subnet_ids" => "subnet-0123456789abcdef0".to_string(),
        "suffix" => "index.html".to_string(),
        "target_prefix" => "logs/".to_string(),
        "transit_gateway_id" => "tgw-0123456789abcdef0".to_string(),
        "value" if resource_type == "route53.RecordSet" => "192.0.2.10".to_string(),
        "vpn_gateway_id" => "vgw-0123456789abcdef0".to_string(),
        _ => "send-contract-value".to_string(),
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
            let first_writable = fields
                .iter()
                .find(|field| !field.parts().declared_input_mode.is_provider_populated())
                .map(|field| field.name());
            for field in fields {
                let parts = field.parts();
                if parts.declared_input_mode.is_provider_populated()
                    || (!parts.declared_input_mode.is_required()
                        && (has_required || Some(parts.name) != first_writable))
                {
                    continue;
                }
                path.push(parts.name.to_string());
                map.insert(
                    parts.name.to_string(),
                    sample_value(resource_type, schema, parts.field_type, path, budget),
                );
                path.pop();
            }
            Value::Concrete(ConcreteValue::Map(map))
        }
        Shape::Union => {
            let members = schema
                .union_members_with_budget(attribute_type, budget)
                .expect("send-contract union walk must stay within its budget");
            let member = members
                .iter()
                .flatten()
                .next()
                .expect("generated send-contract union must have a resolvable member");
            sample_value(resource_type, schema, member.as_attr(), path, budget)
        }
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

fn normalize_request_text(value: &str) -> String {
    let decoded = urlencoding::decode(value)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| value.to_string());
    decoded
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn request_haystack(requests: &[MockRequest]) -> String {
    let mut text = String::new();
    for request in requests {
        text.push_str(&request.uri);
        for (name, value) in &request.headers {
            text.push_str(name.as_str());
            if let Ok(value) = value.to_str() {
                text.push_str(value);
            }
        }
        text.push_str(str::from_utf8(&request.body).unwrap_or_default());
    }
    normalize_request_text(&text)
}

#[tokio::test]
async fn every_sent_attribute_reaches_a_captured_create_request() {
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
        for attribute in contract.attributes {
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
            let haystack = request_haystack(&requests);
            let mut names = vec![attribute.provider_name];
            names.extend_from_slice(attribute.capture_names);
            assert!(
                names
                    .iter()
                    .map(|name| normalize_request_text(name))
                    .any(|name| haystack.contains(&name)),
                "{}.{} was absent from captured requests (accepted names: {:?}); requests={requests:?}",
                contract.resource_type,
                attribute.name,
                names
            );
        }
    }
}
