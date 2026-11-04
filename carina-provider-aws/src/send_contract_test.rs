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
    CompiledAttributeType, CompiledResourceSchema, SchemaKind, Shape, ShapeWalkBudget,
    ValidatedResourceSchema,
};
use indexmap::IndexMap;
use winterbaume_core::{MockAws, MockRequest, MockResponse, MockService};

use crate::provider::{AwsCreateAttempt, AwsCreateOperation, AwsCreateToken, AwsIssuedCreate};
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
            // These are the synthetic resources addressed by capture_identifier.
            // Reads must find them immediately: this test checks mutation payloads,
            // while eventual consistency and read-back budgets have separate tests.
            let inner = match action {
                "CreateVpc" => "<vpc><vpcId>vpc-0123456789abcdef0</vpcId></vpc>",
                "CreateSubnet" => "<subnet><subnetId>subnet-0123456789abcdef0</subnetId></subnet>",
                "CreateInternetGateway" => {
                    "<internetGateway><internetGatewayId>igw-0123456789abcdef0</internetGatewayId></internetGateway>"
                }
                "DescribeVpcs" => {
                    "<vpcSet><item><vpcId>vpc-0123456789abcdef0</vpcId><cidrBlock>10.0.0.0/16</cidrBlock><instanceTenancy>default</instanceTenancy></item></vpcSet>"
                }
                "DescribeVpcAttribute" => {
                    "<vpcId>vpc-0123456789abcdef0</vpcId><enableDnsSupport><value>true</value></enableDnsSupport><enableDnsHostnames><value>true</value></enableDnsHostnames>"
                }
                "DescribeSubnets" => {
                    "<subnetSet><item><subnetId>subnet-0123456789abcdef0</subnetId><vpcId>vpc-0123456789abcdef0</vpcId><cidrBlock>10.0.0.0/24</cidrBlock></item></subnetSet>"
                }
                "DescribeInternetGateways" => {
                    "<internetGatewaySet><item><internetGatewayId>igw-0123456789abcdef0</internetGatewayId><attachmentSet><item><vpcId>vpc-0123456789abcdef0</vpcId><state>available</state></item></attachmentSet></item></internetGatewaySet>"
                }
                "DescribeRouteTables" => {
                    "<routeTableSet><item><routeTableId>rtb-0123456789abcdef0</routeTableId><vpcId>vpc-0123456789abcdef0</vpcId><routeSet><item><destinationCidrBlock>10.0.0.0/24</destinationCidrBlock><gatewayId>igw-0123456789abcdef0</gatewayId></item></routeSet><associationSet><item><routeTableAssociationId>rtbassoc-0123456789abcdef0</routeTableAssociationId><routeTableId>rtb-0123456789abcdef0</routeTableId><subnetId>subnet-0123456789abcdef0</subnetId></item></associationSet></item></routeTableSet>"
                }
                "DescribeSecurityGroups" => {
                    "<securityGroupInfo><item><groupId>sg-0123456789abcdef0</groupId><groupName>send-contract</groupName><groupDescription>send contract</groupDescription><vpcId>vpc-0123456789abcdef0</vpcId></item></securityGroupInfo>"
                }
                "DescribeSecurityGroupRules" => {
                    "<securityGroupRuleSet><item><securityGroupRuleId>sgr-0123456789abcdef0</securityGroupRuleId><groupId>sg-0123456789abcdef0</groupId><isEgress>false</isEgress><ipProtocol>tcp</ipProtocol></item><item><securityGroupRuleId>sgr-0123456789abcdef1</securityGroupRuleId><groupId>sg-0123456789abcdef0</groupId><isEgress>true</isEgress><ipProtocol>tcp</ipProtocol></item></securityGroupRuleSet>"
                }
                "DescribeEgressOnlyInternetGateways" => {
                    "<egressOnlyInternetGatewaySet><item><egressOnlyInternetGatewayId>eigw-0123456789abcdef0</egressOnlyInternetGatewayId><attachmentSet><item><vpcId>vpc-0123456789abcdef0</vpcId><state>attached</state></item></attachmentSet></item></egressOnlyInternetGatewaySet>"
                }
                "DescribeAddresses" => {
                    "<addressesSet><item><allocationId>eipalloc-0123456789abcdef0</allocationId><publicIp>198.51.100.10</publicIp><domain>vpc</domain></item></addressesSet>"
                }
                "DescribeFlowLogs" => {
                    "<flowLogSet><item><flowLogId>fl-0123456789abcdef0</flowLogId><resourceId>vpc-0123456789abcdef0</resourceId><trafficType>ALL</trafficType><flowLogStatus>ACTIVE</flowLogStatus></item></flowLogSet>"
                }
                "DescribeNatGateways" => {
                    "<natGatewaySet><item><natGatewayId>nat-0123456789abcdef0</natGatewayId><subnetId>subnet-0123456789abcdef0</subnetId><state>available</state><connectivityType>public</connectivityType></item></natGatewaySet>"
                }
                "DescribeTransitGateways" => {
                    "<transitGatewaySet><item><transitGatewayId>tgw-0123456789abcdef0</transitGatewayId><state>available</state></item></transitGatewaySet>"
                }
                "DescribeTransitGatewayVpcAttachments" => {
                    "<transitGatewayVpcAttachments><item><transitGatewayAttachmentId>tgw-attach-0123456789abcdef0</transitGatewayAttachmentId><transitGatewayId>tgw-0123456789abcdef0</transitGatewayId><vpcId>vpc-0123456789abcdef0</vpcId><subnetIds><item>subnet-0123456789abcdef0</item></subnetIds><state>available</state></item></transitGatewayVpcAttachments>"
                }
                "DescribeVpcEndpoints" => {
                    "<vpcEndpointSet><item><vpcEndpointId>vpce-0123456789abcdef0</vpcEndpointId><vpcId>vpc-0123456789abcdef0</vpcId><vpcEndpointType>Gateway</vpcEndpointType><serviceName>com.amazonaws.us-east-1.s3</serviceName><routeTableIdSet><item>rtb-0123456789abcdef0</item></routeTableIdSet><subnetIdSet><item>subnet-0123456789abcdef0</item></subnetIdSet><groupSet><item><groupId>sg-0123456789abcdef0</groupId></item></groupSet><state>available</state></item></vpcEndpointSet>"
                }
                "DescribeVpcPeeringConnections" => {
                    "<vpcPeeringConnectionSet><item><vpcPeeringConnectionId>pcx-0123456789abcdef0</vpcPeeringConnectionId><requesterVpcInfo><vpcId>vpc-0123456789abcdef0</vpcId></requesterVpcInfo><accepterVpcInfo><vpcId>vpc-0123456789abcdef1</vpcId></accepterVpcInfo></item></vpcPeeringConnectionSet>"
                }
                "DescribeVpnGateways" => {
                    "<vpnGatewaySet><item><vpnGatewayId>vgw-0123456789abcdef0</vpnGatewayId><type>ipsec.1</type><state>available</state></item></vpnGatewaySet>"
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
            if request.method == "GET" {
                let query = request
                    .uri
                    .split_once('?')
                    .map(|(_, query)| query)
                    .unwrap_or_default();
                let parameter = |name: &str| {
                    query
                        .split('&')
                        .find_map(|part| {
                            let (key, value) = part.split_once('=')?;
                            (key == name).then(|| {
                                urlencoding::decode(value)
                                    .expect("capture query")
                                    .into_owned()
                            })
                        })
                        .expect("record read cursor")
                };
                let name = parameter("name");
                let record_type = parameter("type");
                let value = if record_type == "AAAA" {
                    "2001:db8::1"
                } else {
                    "198.51.100.10"
                };
                return MockResponse::xml(
                    200,
                    format!(
                        r#"<ListResourceRecordSetsResponse xmlns="https://route53.amazonaws.com/doc/2013-04-01/"><ResourceRecordSets><ResourceRecordSet><Name>{name}</Name><Type>{record_type}</Type><TTL>600</TTL><ResourceRecords><ResourceRecord><Value>{value}</Value></ResourceRecord></ResourceRecords></ResourceRecordSet></ResourceRecordSets><IsTruncated>false</IsTruncated><MaxItems>100</MaxItems></ListResourceRecordSetsResponse>"#
                    ),
                );
            }
            return MockResponse::xml(
                200,
                r#"<ChangeResourceRecordSetsResponse xmlns="https://route53.amazonaws.com/doc/2013-04-01/"><ChangeInfo><Id>/change/C1</Id><Status>PENDING</Status><SubmittedAt>2026-01-01T00:00:00Z</SubmittedAt></ChangeInfo></ChangeResourceRecordSetsResponse>"#,
            );
        }
        if request.uri.contains("iam.") {
            let inner = if action == "GetRole" {
                "<Role><Path>/</Path><RoleName>CarinaSendContract</RoleName><RoleId>AROASENDCONTRACT</RoleId><Arn>arn:aws:iam::123456789012:role/CarinaSendContract</Arn><CreateDate>2026-01-01T00:00:00Z</CreateDate><AssumeRolePolicyDocument>{\"Version\":\"2012-10-17\",\"Statement\":[]}</AssumeRolePolicyDocument></Role>"
            } else {
                ""
            };
            return MockResponse::xml(
                200,
                format!(
                    "<{action}Response xmlns=\"https://iam.amazonaws.com/doc/2010-05-08/\"><{action}Result>{inner}</{action}Result></{action}Response>"
                ),
            );
        }
        if request.uri.contains("s3.") || request.uri.contains(".s3.") {
            if request.method == "GET" {
                let query = request
                    .uri
                    .split_once('?')
                    .map(|(_, query)| query)
                    .unwrap_or_default();
                let body = match query
                    .split('&')
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('=')
                {
                    "versioning" => {
                        "<VersioningConfiguration><Status>Enabled</Status></VersioningConfiguration>"
                    }
                    "object-lock" => {
                        "<ObjectLockConfiguration><ObjectLockEnabled>Enabled</ObjectLockEnabled></ObjectLockConfiguration>"
                    }
                    "publicAccessBlock" => "<PublicAccessBlockConfiguration/>",
                    "logging" => {
                        "<BucketLoggingStatus><LoggingEnabled><TargetBucket>carina-log-bucket</TargetBucket><TargetPrefix>logs/</TargetPrefix></LoggingEnabled></BucketLoggingStatus>"
                    }
                    "ownershipControls" => {
                        "<OwnershipControls><Rule><ObjectOwnership>BucketOwnerEnforced</ObjectOwnership></Rule></OwnershipControls>"
                    }
                    "encryption" => {
                        "<ServerSideEncryptionConfiguration><Rule><ApplyServerSideEncryptionByDefault><SSEAlgorithm>AES256</SSEAlgorithm></ApplyServerSideEncryptionByDefault></Rule></ServerSideEncryptionConfiguration>"
                    }
                    "replication" => {
                        "<ReplicationConfiguration><Role>arn:aws:iam::123456789012:role/replication</Role></ReplicationConfiguration>"
                    }
                    "lifecycle" => "<LifecycleConfiguration/>",
                    "website" => {
                        "<WebsiteConfiguration><IndexDocument><Suffix>index.html</Suffix></IndexDocument></WebsiteConfiguration>"
                    }
                    "cors" => "<CORSConfiguration/>",
                    "notification" => "<NotificationConfiguration/>",
                    "tagging" => "<Tagging><TagSet/></Tagging>",
                    "acl" => {
                        "<AccessControlPolicy><Owner><ID>123456789012</ID></Owner><AccessControlList/></AccessControlPolicy>"
                    }
                    "policy" => {
                        return MockResponse::json(
                            200,
                            r#"{"Version":"2012-10-17","Statement":[]}"#,
                        );
                    }
                    _ => "",
                };
                return MockResponse::xml(200, body);
            }
            return MockResponse::xml(200, "");
        }
        if target.ends_with("RequestCertificate") {
            return MockResponse::json(
                200,
                r#"{"CertificateArn":"arn:aws:acm:us-east-1:123456789012:certificate/00000000-0000-0000-0000-000000000000"}"#,
            );
        }
        if target.ends_with("DescribeCertificate") {
            return MockResponse::json(
                200,
                r#"{"Certificate":{"CertificateArn":"arn:aws:acm:us-east-1:123456789012:certificate/00000000-0000-0000-0000-000000000000","DomainName":"send-contract.example.com","Status":"ISSUED","KeyAlgorithm":"RSA_2048"}}"#,
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
        if target.ends_with("DescribeAccount") {
            return MockResponse::json(
                200,
                r#"{"Account":{"Id":"123456789012","Arn":"arn:aws:organizations::123456789012:account/o-exampleorgid/123456789012","Name":"send-contract","Email":"send-contract@example.com","Status":"ACTIVE"}}"#,
            );
        }
        if target.ends_with("DescribeLogGroups") {
            let body: serde_json::Value =
                serde_json::from_str(body).expect("log group read request");
            return MockResponse::json(
                200,
                serde_json::json!({"logGroups": [{"logGroupName": body["logGroupNamePrefix"], "logGroupClass": "STANDARD"}]}).to_string(),
            );
        }
        if target.ends_with("CreateQueue") {
            return MockResponse::json(
                200,
                r#"{"QueueUrl":"https://sqs.us-east-1.amazonaws.com/123456789012/carina-send-contract.fifo"}"#,
            );
        }
        if target.ends_with("GetQueueAttributes") {
            return MockResponse::json(
                200,
                r#"{"Attributes":{"VisibilityTimeout":"30","MessageRetentionPeriod":"345600","DelaySeconds":"0","MaximumMessageSize":"1048576","ReceiveMessageWaitTimeSeconds":"0","QueueArn":"arn:aws:sqs:us-east-1:123456789012:carina-send-contract.fifo","CreatedTimestamp":"1767225600","LastModifiedTimestamp":"1767225600","FifoQueue":"true"}}"#,
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

    fn not_created(self, error: ProviderError) -> Self::Verdict {
        error
    }

    fn issued(self, identifier: AppliedIdentifier, _sent: Sent) -> Self::Issued {
        CaptureIssued { identifier }
    }

    fn pending(
        self,
        _token: AwsCreateOperation,
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
        _reason: ProviderError,
        _missing_attributes: crate::NonEmptyAttributeNames,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(())
    }

    fn identifier_reported(self, _observation: Observation, error: ProviderError) -> Self::Verdict {
        error
    }

    fn no_resource(self, error: ProviderError) -> Self::Verdict {
        error
    }

    fn pending(
        self,
        _token: AwsCreateToken,
        _poll_after_secs: u32,
    ) -> Result<Self::Step, Self::Verdict> {
        Ok(())
    }

    fn preserve(self, _token: AwsCreateToken, error: ProviderError) -> Self::Verdict {
        error
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
        "dead_letter_target_arn" => {
            format!("arn:aws:sqs:{REGION}:123456789012:send-contract-{suffix}.fifo")
        }
        "dns_name" | "domain_name" | "host_name" | "name" => {
            format!("send-{suffix}.example.com")
        }
        "email" => format!("send-{suffix}@example.com"),
        "gateway_id" | "internet_gateway_id" => ec2_id("igw"),
        "group_id"
        | "security_group_ids"
        | "destination_security_group_id"
        | "source_security_group_id" => ec2_id("sg"),
        "group_name" => format!("send-contract-{suffix}"),
        "hosted_zone_id" => format!("Z{hash:016X}"),
        "ipv4_ipam_pool_id" | "ipv6_ipam_pool_id" => ec2_id("ipam-pool"),
        "kms_key_id" | "kms_master_key_id" => {
            format!("arn:aws:kms:{REGION}:123456789012:key/12345678-1234-1234-1234-0000{suffix}")
        }
        "log_group_name" => format!("/carina/send-{suffix}"),
        "nat_gateway_id" => ec2_id("nat"),
        "outpost_arn" => {
            "arn:aws:outposts:us-east-1:123456789012:outpost/op-0123456789abcdef0".to_string()
        }
        "path" => "/".to_string(),
        "destination_prefix_list_id" | "source_prefix_list_id" => ec2_id("pl"),
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
    schema: &CompiledResourceSchema,
    attribute_type: &CompiledAttributeType,
    path: &mut Vec<String>,
    budget: &mut ShapeWalkBudget,
) -> Value {
    match schema.shape_of(attribute_type) {
        Shape::UnloadedSchemaType(unloaded) => panic!(
            "send-contract samples need a loaded schema type for {} at {}",
            unloaded.identity(),
            path.join("."),
        ),
        Shape::String { .. } | Shape::Reference { .. } => {
            string(sample_string(resource_type, path))
        }
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
    resource.set_attr(
        name,
        attribute
            .canonicalize_state(value)
            .into_present()
            .expect("generated sample must remain present"),
    );
}

fn set_bool(resource: &mut Resource, name: &str, value: bool) {
    resource.set_attr(name, Value::Concrete(ConcreteValue::Bool(value)));
}

fn set_canonical_value(
    resource: &mut Resource,
    schema: &ValidatedResourceSchema,
    name: &str,
    value: Value,
) {
    let attribute = schema.effective_attribute(name).unwrap_or_else(|| {
        panic!(
            "{} has no schema attribute {name}",
            resource.id.resource_type
        )
    });
    resource.set_attr(
        name,
        attribute
            .canonicalize_state(value)
            .into_present()
            .expect("canonical fixture must remain present"),
    );
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
                set_canonical_value(&mut resource, schema, "availability_mode", string("zonal"));
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
            set_canonical_value(&mut resource, schema, "validation_method", string("EMAIL"));
            if target.name == "options" {
                // The generic struct sampler chooses one optional member. Pin
                // this focused case to every CertificateOptions member so
                // restoring ValidationMethod in #580 does not trade away CT
                // logging or export capture coverage.
                set_canonical_value(
                    &mut resource,
                    schema,
                    "options",
                    Value::Concrete(ConcreteValue::Map(IndexMap::from([
                        (
                            "certificate_transparency_logging_preference".to_string(),
                            string("DISABLED"),
                        ),
                        ("export".to_string(), string("ENABLED")),
                        ("validation_method".to_string(), string("DNS")),
                    ]))),
                );
            }
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
        "organizations.Organization" => "o-exampleorgid",
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
        "ec2.SecurityGroupIngress" => "sgr-0123456789abcdef0",
        "ec2.SecurityGroupEgress" => "sgr-0123456789abcdef1",
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
        _ => panic!("missing capture identifier for {resource_type}"),
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

enum MarkerExpectation {
    Any,
    Every,
}

fn assert_sample_reached_mutation(
    contract: &SendCaptureContract,
    attribute: &SendCaptureAttribute,
    resource: &Resource,
    requests: &[MockRequest],
    operation: &str,
    marker_expectation: MarkerExpectation,
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
    if matches!(marker_expectation, MarkerExpectation::Every) {
        let missing_markers = markers
            .iter()
            .filter(|marker| !payload_contains_marker(&payload, marker))
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert!(
            missing_markers.is_empty(),
            "{}.{} missing markers {missing_markers:?} from mutating {operation} URI/query/body; required markers={markers:?}; mutating payload={payload:?}; requests={requests:?}",
            contract.resource_type,
            attribute.name,
        );
        return;
    }
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

async fn prepare_create_for_capture(
    mut resource: Resource,
    bindings: &carina_core::binding_index::ResolvedBindings,
) -> Result<carina_core::provider::CreateRequest, carina_core::executor::ProviderPreparationError> {
    carina_core::validation::deferred_types::with_session_for_test_async(
        &carina_core::parser::LoadedProviderContext::empty_for_test(),
        async |type_checks| {
            carina_core::resolver::resolve_refs_with_state_and_remote(
                std::slice::from_mut(&mut resource),
                bindings,
                type_checks,
            )
            .expect("capture fixture references should resolve");
            let gate = carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
            let schemas = crate::schemas::schema_registry();
            let factories: Vec<Box<dyn carina_core::provider::ProviderFactory>> =
                vec![Box::new(crate::AwsProviderFactory)];
            let preparation = carina_core::executor::ProviderPreparationContext::new(
                bindings,
                &gate,
                &[],
                &crate::AwsNormalizer,
                &factories,
                &schemas,
                type_checks,
            );
            carina_core::executor::prepare_create_request(resource, &preparation).await
        },
    )
    .await
}

#[tokio::test]
async fn native_create_preparation_rejects_sqs_and_nat_cross_field_conflicts_before_send() {
    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(mock.sdk_config(REGION).await),
    );
    let bindings = carina_core::binding_index::ResolvedBindings::default();
    let sqs = || Resource::with_provider("aws", "sqs.Queue", "invalid-create", None);
    let canonicalize_sqs = |mut resource: Resource| {
        for (name, field_type) in [
            (
                "deduplication_scope",
                carina_aws_types::sqs_queue_deduplication_scope(),
            ),
            (
                "fifo_throughput_limit",
                carina_aws_types::sqs_queue_fifo_throughput_limit(),
            ),
            (
                "redrive_allow_policy",
                carina_aws_types::sqs_redrive_allow_policy(),
            ),
        ] {
            if let Some(value) = resource.get_attr(name).cloned() {
                resource.set_attr(
                    name,
                    crate::test_rootless_type(field_type)
                        .canonicalize_state(value)
                        .into_present()
                        .expect("SQS enum fixture remains present"),
                );
            }
        }
        resource
    };
    let sources = |count| {
        Value::Concrete(ConcreteValue::List(
            (0..count)
                .map(|i| string(format!("arn:aws:sqs:us-east-1:123456789012:source-{i}")))
                .collect(),
        ))
    };
    let redrive = |permission: &str, count| {
        let fields = IndexMap::from([
            ("redrive_permission".into(), string(permission)),
            ("source_queue_arns".into(), sources(count)),
        ]);
        sqs()
            .with_attribute("queue_name", string("standard-queue"))
            .with_attribute(
                "redrive_allow_policy",
                Value::Concrete(ConcreteValue::Map(fields)),
            )
    };
    for (resource, message) in [
        (
            Resource::with_provider("aws", "sqs.Queue", "invalid-create", None)
                .with_attribute("queue_name", string("invalid-create"))
                .with_attribute("fifo_queue", Value::Concrete(ConcreteValue::Bool(false)))
                .with_attribute(
                    "content_based_deduplication",
                    Value::Concrete(ConcreteValue::Bool(true)),
                ),
            "content_based_deduplication requires fifo_queue = true",
        ),
        (
            sqs()
                .with_attribute("queue_name", string("private-name"))
                .with_attribute("fifo_queue", Value::Concrete(ConcreteValue::Bool(true))),
            "queue_name must end with .fifo",
        ),
        (
            sqs()
                .with_attribute("queue_name", string("valid.fifo"))
                .with_attribute("fifo_queue", Value::Concrete(ConcreteValue::Bool(true)))
                .with_attribute("fifo_throughput_limit", string("perMessageGroupId"))
                .with_attribute("deduplication_scope", string("queue")),
            "fifo_throughput_limit requires deduplication_scope = messageGroup",
        ),
        (
            redrive("allowAll", 1),
            "redrive_allow_policy.source_queue_arns may only be set",
        ),
        (
            redrive("byQueue", 0),
            "redrive_allow_policy.redrive_permission requires redrive_allow_policy.source_queue_arns",
        ),
        (
            redrive("byQueue", 11),
            "redrive_allow_policy.source_queue_arns must contain at most 10 items",
        ),
        (
            Resource::with_provider("aws", "ec2.NatGateway", "invalid-create", None),
            "subnet_id is required when availability_mode = zonal",
        ),
    ] {
        let resource = canonicalize_sqs(resource);
        let id = resource.id.clone();
        let from = crate::test_existing_state(
            id.clone(),
            resource.attributes.clone().into_iter().collect(),
        )
        .to_snapshot();
        let error = match prepare_create_for_capture(resource, &bindings).await {
            Err(carina_core::executor::ProviderPreparationError::Provider(error)) => error,
            Err(error) => {
                panic!("cross-field validation must reject during normalization: {error:?}")
            }
            Ok(request) => {
                let (create, _cancellation) =
                    carina_core::executor::dispatch_create(&provider, &id, request);
                let _ = tokio::time::timeout(Duration::from_secs(1), create).await;
                panic!("invalid native create passed preparation and reached the provider");
            }
        };
        assert!(
            service.snapshot().is_empty(),
            "invalid create must not send any AWS request"
        );
        assert!(
            matches!(&error, ProviderError::InvalidInput(_)),
            "{error:?}"
        );
        assert!(error.to_string().contains(message), "{error:?}");
        assert!(!error.to_string().contains("private-name"));
        assert!(!error.to_string().contains("arn:aws:sqs"));
        if id.resource_type == "sqs.Queue" {
            let identifier = AppliedIdentifier::new(capture_identifier("sqs.Queue")).unwrap();
            let patch = UpdatePatch {
                ops: vec![PatchOp::Add {
                    key: "delay_seconds".into(),
                    value: Value::Concrete(ConcreteValue::Int(1)),
                }],
            };
            let error = provider
                .update_step(&id, &identifier, &from, &patch)
                .await
                .expect_err(
                    "native update must materialize and validate unchanged cross-field inputs",
                );
            assert!(matches!(error, ProviderError::InvalidInput(_)));
            assert!(error.to_string().contains(message), "{error:?}");
            assert!(
                service.snapshot().is_empty(),
                "invalid update must not send any AWS request"
            );
        }
    }

    let resource = redrive("byQueue", 1)
        .with_attribute("queue_name", string("valid.fifo"))
        .with_attribute("fifo_queue", Value::Concrete(ConcreteValue::Bool(true)))
        .with_attribute("fifo_throughput_limit", string("perMessageGroupId"))
        .with_attribute("deduplication_scope", string("messageGroup"));
    prepare_create_for_capture(canonicalize_sqs(resource), &bindings)
        .await
        .expect("valid SQS combinations must pass native create preparation");
}

#[tokio::test]
async fn sqs_tags_only_update_preserves_unconsumed_fifo_secret_hashes() {
    use carina_core::resource::DeferredValue;

    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(mock.sdk_config(REGION).await),
    );
    let identifier = AppliedIdentifier::new(capture_identifier("sqs.Queue")).unwrap();
    let resource = Resource::with_provider("aws", "sqs.Queue", "fifo-secrets", None)
        .with_attribute("queue_name", string("fifo-secrets.fifo"))
        .with_attribute("fifo_queue", Value::Concrete(ConcreteValue::Bool(true)))
        .with_attribute("deduplication_scope", string("messageGroup"))
        .with_attribute("fifo_throughput_limit", string("perMessageGroupId"));
    let patch = UpdatePatch {
        ops: vec![PatchOp::Add {
            key: "tags".into(),
            value: Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "Environment".into(),
                string("new"),
            )]))),
        }],
    };

    for attribute in ["deduplication_scope", "fifo_throughput_limit"] {
        let mut from = existing_state_without(&resource, None).to_snapshot();
        from.attributes.insert(
            attribute.into(),
            carina_core::value::redact_secrets_in_value(&Value::Deferred(DeferredValue::Secret(
                Box::new(resource.get_attr(attribute).unwrap().clone()),
            )))
            .expect("FIFO create-only secret should persist as a hash"),
        );

        service.clear();
        provider
            .update_step(&resource.id, &identifier, &from, &patch)
            .await
            .unwrap_or_else(|error| {
                panic!("tags-only update must not consume {attribute}: {error:?}")
            });
        let requests = service.snapshot();
        let mutations = requests
            .iter()
            .filter(|request| is_mutating_request(request))
            .collect::<Vec<_>>();
        assert_eq!(mutations.len(), 1, "{requests:?}");
        assert_eq!(
            mutations[0]
                .headers
                .get("x-amz-target")
                .and_then(|value| value.to_str().ok()),
            Some("AmazonSQS.TagQueue")
        );
    }
}

#[tokio::test]
async fn sqs_update_rejects_fifo_conflicts_after_deferred_inputs_resolve_before_send() {
    use carina_core::binding_index::{BindingValueSource, ResolvedBindings};
    use carina_core::provider::ProviderNormalizer;
    use carina_core::resource::DeferredValue;

    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(mock.sdk_config(REGION).await),
    );
    let identifier =
        AppliedIdentifier::new("https://sqs.us-east-1.amazonaws.com/123456789012/resolved-queue")
            .unwrap();
    let mut bindings = ResolvedBindings::default();
    bindings.set(
        "settings",
        HashMap::from([(
            "fifo_queue".into(),
            Value::Concrete(ConcreteValue::Bool(false)),
        )]),
        BindingValueSource::Local,
    );

    for fifo in [
        Value::resource_ref("settings", "fifo_queue", vec![]).resolve_root_for_test(),
        Value::Deferred(DeferredValue::Secret(Box::new(Value::Concrete(
            ConcreteValue::Bool(false),
        )))),
    ] {
        let desired = Resource::with_provider("aws", "sqs.Queue", "resolved-queue", None)
            .with_attribute("queue_name", string("resolved-queue"))
            .with_attribute("fifo_queue", fifo)
            .with_attribute(
                "content_based_deduplication",
                Value::Concrete(ConcreteValue::Bool(true)),
            );
        let id = desired.id.clone();
        let mut resources = vec![desired.clone().into()];
        crate::AwsNormalizer
            .normalize_desired(&mut resources)
            .await
            .expect("plan-time validation must defer the opaque FIFO flag");

        // Prepare the queue's previous settings through real reference resolution
        // and secret unwrapping. The update retains that resolved FIFO flag.
        let mut previous = desired;
        previous.set_attr(
            "content_based_deduplication",
            Value::Concrete(ConcreteValue::Bool(false)),
        );
        let request = prepare_create_for_capture(previous, &bindings)
            .await
            .expect("the previous queue settings must resolve and pass preparation");
        let from = crate::test_existing_state(
            id.clone(),
            request.resource().as_resource().resolved_attributes(),
        );
        assert_eq!(
            from.attributes.get("fifo_queue"),
            Some(&Value::Concrete(ConcreteValue::Bool(false)))
        );
        let patch = UpdatePatch {
            ops: vec![PatchOp::Replace {
                key: "content_based_deduplication".into(),
                value: resources[0]
                    .get_attr("content_based_deduplication")
                    .unwrap()
                    .clone(),
            }],
        };
        service.clear();
        let result = provider.update_step(&id, &identifier, &from, &patch).await;
        assert!(
            service.snapshot().is_empty(),
            "resolved FIFO conflicts must fail before any AWS request"
        );
        let error = result.expect_err("the fully resolved update must be validated");
        assert!(
            matches!(&error, ProviderError::InvalidInput(_)),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("content_based_deduplication requires fifo_queue = true"),
            "{error:?}"
        );
    }
}

#[tokio::test]
async fn acm_tag_update_does_not_restore_outputs_omitted_by_final_read_back() {
    use carina_core::resource::DeferredValue;
    use carina_provider_protocol::ReadBackEnd;
    use carina_provider_protocol::types::{OperationConfig, ReadBackBudget};

    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(mock.sdk_config(REGION).await),
    )
    .with_operation_config_for_test(
        "acm.Certificate",
        OperationConfig {
            read_back: ReadBackBudget::from_secs(1),
            ..Default::default()
        },
    );
    let resource: Resource = Resource::with_provider("aws", "acm.Certificate", "cached-dvo", None);
    let id = resource.id;
    let mut from = crate::test_existing_state(
        id.clone(),
        HashMap::from([
            ("domain_name".into(), string("send-contract.example.com")),
            ("validation_method".into(), string("DNS")),
            (
                "domain_validation_options".into(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(IndexMap::from([
                        ("domain_name".into(), string("send-contract.example.com")),
                        (
                            "resource_record".into(),
                            Value::Concrete(ConcreteValue::Map(IndexMap::from([
                                ("name".into(), string("_old.send-contract.example.com.")),
                                ("type".into(), string("CNAME")),
                                ("value".into(), string("_old.acm-validations.aws.")),
                            ]))),
                        ),
                    ])),
                )])),
            ),
            ("failure_reason".into(), string("NO_AVAILABLE_CONTACTS")),
        ]),
    )
    .to_snapshot();
    let options_snapshot =
        carina_core::value::redact_secrets_in_value(&Value::Deferred(DeferredValue::Secret(
            Box::new(Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "certificate_transparency_logging_preference".into(),
                string("ENABLED"),
            )])))),
        )))
        .expect("persisted create-only options");
    from.attributes
        .insert("options".into(), options_snapshot.clone());
    let patch = UpdatePatch {
        ops: vec![PatchOp::Add {
            key: "tags".into(),
            value: Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "Environment".into(),
                string("new"),
            )]))),
        }],
    };
    let identifier = AppliedIdentifier::new(capture_identifier("acm.Certificate")).unwrap();

    let outcome =
        provider
            .update_with_poll_sleep_for_test(&id, &identifier, &from, &patch, |_| {
                std::future::ready(())
            })
            .await
            .expect("tag update should finish with a partial read-back");

    assert_eq!(outcome.read_back_end(), ReadBackEnd::HostDeadline);
    assert_eq!(
        outcome.diagnostic().unwrap().missing_attributes(),
        ["domain_validation_options"]
    );
    let state = outcome.into_state_for_writeback();
    assert!(
        !state.attributes.contains_key("domain_validation_options"),
        "omitted DNS records must remain absent so cached planning can mark them unknown"
    );
    assert!(
        !state.attributes.contains_key("failure_reason"),
        "other outputs omitted by the final read must also stay absent"
    );
    assert!(state.attributes.contains_key("status"));
    assert_eq!(state.attributes.get("options"), Some(&options_snapshot));
    let requests = service.snapshot();
    for (target, expected) in [
        ("CertificateManager.AddTagsToCertificate", 1),
        ("CertificateManager.DescribeCertificate", 2),
    ] {
        assert_eq!(
            requests
                .iter()
                .filter(|request| {
                    request
                        .headers
                        .get("x-amz-target")
                        .and_then(|v| v.to_str().ok())
                        == Some(target)
                })
                .count(),
            expected,
            "{target}"
        );
    }
}

#[tokio::test]
async fn organizations_account_tag_update_preserves_unused_secret_role_snapshot() {
    use carina_core::resource::DeferredValue;

    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(mock.sdk_config(REGION).await),
    );
    let desired: Resource =
        Resource::with_provider("aws", "organizations.Account", "secret-role", None);
    let id = desired.id;
    let mut from = crate::test_existing_state(
        id.clone(),
        HashMap::from([
            ("account_name".into(), string("send-contract")),
            ("email".into(), string("send-contract@example.com")),
            (
                "tags".into(),
                Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                    "Environment".into(),
                    string("old"),
                )]))),
            ),
        ]),
    )
    .to_snapshot();
    let role_snapshot = carina_core::value::redact_secrets_in_value(&Value::Deferred(
        DeferredValue::Secret(Box::new(string("Role"))),
    ))
    .expect("persisted secret role name");
    from.attributes
        .insert("role_name".into(), role_snapshot.clone());
    let patch = UpdatePatch {
        // Create-only role_name has no Reassert, even when authored as secret("Role").
        ops: vec![PatchOp::Replace {
            key: "tags".into(),
            value: Value::Concrete(ConcreteValue::Map(IndexMap::from([(
                "Environment".into(),
                string("new"),
            )]))),
        }],
    };
    let identifier = AppliedIdentifier::new("123456789012").expect("account identifier");

    provider
        .update_with_patch(&id, &identifier, &from, &patch)
        .await
        .expect("tag update must not require the create-only role name's plaintext");

    let requests = service.snapshot();
    let tag_request = requests
        .iter()
        .find(|request| {
            request.headers["x-amz-target"]
                .to_str()
                .expect("AWS JSON target")
                .ends_with(".TagResource")
        })
        .expect("tag update must reach AWS");
    let body: serde_json::Value =
        serde_json::from_slice(&tag_request.body).expect("tag request JSON");
    assert_eq!(body["ResourceId"], "123456789012");
    assert_eq!(
        body["Tags"],
        serde_json::json!([{"Key": "Environment", "Value": "new"}])
    );
    assert_eq!(from.attributes.get("role_name"), Some(&role_snapshot));
}

#[tokio::test]
async fn route53_update_uses_identifier_type_and_sends_mutable_attributes() {
    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(mock.sdk_config(REGION).await),
    );
    let schema = crate::test_resource_schema("route53.RecordSet");

    for desired_type in [None, Some("CNAME")] {
        service.clear();
        let mut desired = Resource::with_provider("aws", "route53.RecordSet", "test-record", None);
        desired.set_attr("ttl", Value::Concrete(ConcreteValue::Int(600)));
        desired.set_attr(
            "resource_records",
            Value::Concrete(ConcreteValue::List(vec![string("2001:db8::1")])),
        );
        if let Some(desired_type) = desired_type {
            desired.set_attr("type", string(desired_type));
        }
        let identifier = crate::helpers::applied_identifier(
            &desired.id,
            "test.Route53Update",
            "Z0123456789ABCDEF|record.example.com|AAAA".to_string(),
        )
        .expect("applied record identifier");

        let from = crate::test_existing_state(desired.id.clone(), desired.resolved_attributes())
            .to_snapshot();
        let update = crate::UpdateResource::new(&from, &UpdatePatch { ops: vec![] });
        let result = provider
            .update_route53_record_set(desired.id.clone(), &identifier, &update, schema)
            .await;
        // Also check that read-back addresses the identifier's record type.
        assert!(result.is_ok(), "update must reach read-back: {result:?}");
        let requests = service.snapshot();
        let mutation = requests
            .iter()
            .find(|request| request.method == "POST")
            .expect("record update must send a mutation");
        assert!(mutation.uri.contains("/hostedzone/Z0123456789ABCDEF/rrset"));
        let body = str::from_utf8(&mutation.body).expect("Route 53 XML request");
        for expected in [
            "<Action>UPSERT</Action>",
            "<Name>record.example.com.</Name>",
            "<Type>AAAA</Type>",
            "<TTL>600</TTL>",
            "<Value>2001:db8::1</Value>",
        ] {
            assert!(body.contains(expected), "missing {expected}: {body}");
        }
        assert!(
            !body.contains("CNAME"),
            "create-only type leaked into update"
        );
        let read = requests
            .iter()
            .find(|request| request.method == "GET")
            .expect("record update must read back the existing record");
        assert!(read.uri.contains("type=AAAA"), "{}", read.uri);
    }
}

#[tokio::test]
async fn every_sent_attribute_reaches_captured_create_and_update_mutations() {
    // Keep Tokio time live so accidental real sleeps fail within 10s, including
    // short sleeps accumulated across the resource/attribute matrix.
    tokio::time::timeout(
        Duration::from_secs(10),
        Box::pin(capture_create_and_update_mutations()),
    )
    .await
    .expect(
        "mutation capture exceeded 10s; capture reads must return the target resource immediately",
    );
}

async fn capture_create_and_update_mutations() {
    let service = Arc::new(CaptureService::default());
    let mock = MockAws::builder()
        .with_service(Arc::clone(&service))
        .build();
    let provider = AwsProvider::from_sdk_config(
        REGION.to_string(),
        vec![],
        vec![],
        &BoundedSdkConfig::for_test(mock.sdk_config(REGION).await),
    );
    let schemas = crate::schemas::all_schemas();

    carina_core::validation::deferred_types::with_session_for_test_async(
        &carina_core::parser::LoadedProviderContext::empty_for_test(),
        async |type_checks| {
            let bindings = carina_core::binding_index::ResolvedBindings::default();
            let gate = carina_core::executor::ModuleConstraintGate::from_live_compositions(&[]);
            let factories: Vec<Box<dyn carina_core::provider::ProviderFactory>> =
                vec![Box::new(crate::AwsProviderFactory)];
            // Reuse the provider's checked schemas across the matrix; compiling
            // the full registry per attribute consumes the capture timeout.
            let preparation = carina_core::executor::ProviderPreparationContext::new(
                &bindings,
                &gate,
                &[],
                &crate::AwsNormalizer,
                &factories,
                &provider.schema_registry,
                type_checks,
            );
            capture_prepared_create_and_update_mutations(
                &provider,
                &service,
                &schemas,
                &preparation,
            )
            .await;
        },
    )
    .await;
}

async fn capture_prepared_create_and_update_mutations(
    provider: &AwsProvider,
    service: &CaptureService,
    schemas: &[ValidatedResourceSchema],
    preparation: &carina_core::executor::ProviderPreparationContext<'_>,
) {
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
            let request =
                carina_core::executor::prepare_create_request(resource.clone(), preparation)
                    .await
                    .unwrap_or_else(|error| {
                        panic!(
                            "{}.{} must pass real create preparation: {error:?}",
                            contract.resource_type, attribute.name,
                        )
                    });
            let result = Box::pin(provider.create_step(request.resource(), CaptureAttempt)).await;
            let requests = service.snapshot();
            assert!(
                !requests.is_empty(),
                "{}.{} failed before reaching the capture transport; result={result:?}; desired={:?}",
                contract.resource_type,
                attribute.name,
                resource.attributes
            );
            let marker_expectation =
                if contract.resource_type == "acm.Certificate" && attribute.name == "options" {
                    MarkerExpectation::Every
                } else {
                    MarkerExpectation::Any
                };
            assert_sample_reached_mutation(
                contract,
                attribute,
                &resource,
                &requests,
                "create",
                marker_expectation,
            );
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
            let result =
                Box::pin(provider.update_with_patch(&resource.id, &identifier, &from, &patch))
                    .await;
            let requests = service.snapshot();
            assert!(
                !requests.is_empty(),
                "{} update failed before reaching the capture transport; target={:?}; result={result:?}; desired={:?}",
                contract.resource_type,
                target.map(|attribute| attribute.name),
                resource.attributes,
            );
            let outcome = result.unwrap_or_else(|error| {
                panic!(
                    "{} capture update failed; target={:?}: {error:?}",
                    contract.resource_type,
                    target.map(|attribute| attribute.name),
                )
            });
            assert!(
                outcome.diagnostic().is_none(),
                "{} capture read must complete immediately; target={:?}; diagnostic={:?}",
                contract.resource_type,
                target.map(|attribute| attribute.name),
                outcome.diagnostic(),
            );
            if let Some(attribute) = target {
                assert_sample_reached_mutation(
                    contract,
                    attribute,
                    &resource,
                    &requests,
                    "update",
                    MarkerExpectation::Any,
                );
            }
        }
    }
}
