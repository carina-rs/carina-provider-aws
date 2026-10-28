//! Resend-safety decisions used by the AWS provider code generator.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use carina_smithy::{ServiceShape, Shape, SmithyModel};
use heck::{ToSnakeCase, ToUpperCamelCase};

const TRAIT_READONLY: &str = "smithy.api#readonly";
const TRAIT_IDEMPOTENT: &str = "smithy.api#idempotent";
const TRAIT_IDEMPOTENCY_TOKEN: &str = "smithy.api#idempotencyToken";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    ReadOnly,
    Idempotent,
    Token(&'static str),
    NotIdempotent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Override {
    pub service: &'static str,
    pub operation: &'static str,
    pub decision: Decision,
    pub justification: &'static str,
}

const fn decision(
    service: &'static str,
    operation: &'static str,
    decision: Decision,
    justification: &'static str,
) -> Override {
    Override {
        service,
        operation,
        decision,
        justification,
    }
}

/// Closed resend-safety decisions for every AWS operation called by the provider.
pub const OVERRIDES: &[Override] = &[
    decision(
        "acm",
        "AddTagsToCertificate",
        Decision::Idempotent,
        "AddTagsToCertificate: sets tag keys on one certificate; repeating the same key/value pairs has no additional effect",
    ),
    decision(
        "acm",
        "DeleteCertificate",
        Decision::NotIdempotent,
        "DeleteCertificate: the provider does not accept an already-absent certificate as success",
    ),
    decision(
        "acm",
        "DescribeCertificate",
        Decision::ReadOnly,
        "DescribeCertificate: reads certificate state and has no side effects",
    ),
    decision(
        "acm",
        "ListTagsForCertificate",
        Decision::ReadOnly,
        "ListTagsForCertificate: reads certificate tags and has no side effects",
    ),
    decision(
        "acm",
        "RemoveTagsFromCertificate",
        Decision::Idempotent,
        "RemoveTagsFromCertificate: removes named tag keys; repeating the removal leaves the same state",
    ),
    decision(
        "acm",
        "RequestCertificate",
        Decision::Token("IdempotencyToken"),
        "RequestCertificate: the IdempotencyToken identifies one certificate request within ACM's token window",
    ),
    decision(
        "acm",
        "UpdateCertificateOptions",
        Decision::Idempotent,
        "UpdateCertificateOptions: replaces the requested certificate option values keyed by certificate ARN",
    ),
    decision(
        "cloudwatchlogs",
        "AssociateKmsKey",
        Decision::Idempotent,
        "AssociateKmsKey: sets the KMS-key association for one log group",
    ),
    decision(
        "cloudwatchlogs",
        "CreateLogGroup",
        Decision::NotIdempotent,
        "CreateLogGroup: creates a top-level resource without an idempotency token",
    ),
    decision(
        "cloudwatchlogs",
        "DeleteLogGroup",
        Decision::NotIdempotent,
        "DeleteLogGroup: the provider does not accept an already-absent log group as success",
    ),
    decision(
        "cloudwatchlogs",
        "DeleteRetentionPolicy",
        Decision::Idempotent,
        "DeleteRetentionPolicy: removes the single retention policy keyed by log group",
    ),
    decision(
        "cloudwatchlogs",
        "DescribeLogGroups",
        Decision::ReadOnly,
        "DescribeLogGroups: reads log-group state and has no side effects",
    ),
    decision(
        "cloudwatchlogs",
        "DisassociateKmsKey",
        Decision::Idempotent,
        "DisassociateKmsKey: removes the KMS-key association from one log group",
    ),
    decision(
        "cloudwatchlogs",
        "ListTagsForResource",
        Decision::ReadOnly,
        "ListTagsForResource: reads resource tags and has no side effects",
    ),
    decision(
        "cloudwatchlogs",
        "ListTagsLogGroup",
        Decision::ReadOnly,
        "ListTagsLogGroup: reads log-group tags and has no side effects",
    ),
    decision(
        "cloudwatchlogs",
        "PutRetentionPolicy",
        Decision::Idempotent,
        "PutRetentionPolicy: replaces the single retention policy keyed by log group",
    ),
    decision(
        "cloudwatchlogs",
        "TagLogGroup",
        Decision::Idempotent,
        "TagLogGroup: sets tag keys on one log group; repeating the same values has no additional effect",
    ),
    decision(
        "cloudwatchlogs",
        "TagResource",
        Decision::Idempotent,
        "TagResource: sets tag keys on one resource; repeating the same values has no additional effect",
    ),
    decision(
        "cloudwatchlogs",
        "UntagLogGroup",
        Decision::Idempotent,
        "UntagLogGroup: removes named tag keys; repeating the removal leaves the same state",
    ),
    decision(
        "cloudwatchlogs",
        "UntagResource",
        Decision::Idempotent,
        "UntagResource: removes named tag keys; repeating the removal leaves the same state",
    ),
    decision(
        "ec2",
        "AllocateAddress",
        Decision::NotIdempotent,
        "AllocateAddress: each successful call can allocate another address and the API has no client token",
    ),
    decision(
        "ec2",
        "AssociateRouteTable",
        Decision::NotIdempotent,
        "AssociateRouteTable: creates an association without an idempotency token",
    ),
    decision(
        "ec2",
        "AttachInternetGateway",
        Decision::NotIdempotent,
        "AttachInternetGateway: attaches a resource and has no idempotency token",
    ),
    decision(
        "ec2",
        "AttachVpnGateway",
        Decision::NotIdempotent,
        "AttachVpnGateway: attaches a resource and has no idempotency token",
    ),
    decision(
        "ec2",
        "AuthorizeSecurityGroupEgress",
        Decision::NotIdempotent,
        "AuthorizeSecurityGroupEgress: authorizes rules without an idempotency token",
    ),
    decision(
        "ec2",
        "AuthorizeSecurityGroupIngress",
        Decision::NotIdempotent,
        "AuthorizeSecurityGroupIngress: authorizes rules without an idempotency token",
    ),
    decision(
        "ec2",
        "CreateEgressOnlyInternetGateway",
        Decision::Token("ClientToken"),
        "CreateEgressOnlyInternetGateway: ClientToken identifies one gateway creation request",
    ),
    decision(
        "ec2",
        "CreateFlowLogs",
        Decision::Token("ClientToken"),
        "CreateFlowLogs: ClientToken identifies one flow-log creation request",
    ),
    decision(
        "ec2",
        "CreateInternetGateway",
        Decision::NotIdempotent,
        "CreateInternetGateway: each successful call creates a new gateway and the API has no client token",
    ),
    decision(
        "ec2",
        "CreateNatGateway",
        Decision::Token("ClientToken"),
        "CreateNatGateway: the model's ClientToken identifies one NAT-gateway creation request",
    ),
    decision(
        "ec2",
        "CreateRoute",
        Decision::NotIdempotent,
        "CreateRoute: creates a route without an idempotency token",
    ),
    decision(
        "ec2",
        "CreateRouteTable",
        Decision::Token("ClientToken"),
        "CreateRouteTable: the model's ClientToken identifies one route-table creation request",
    ),
    decision(
        "ec2",
        "CreateSecurityGroup",
        Decision::NotIdempotent,
        "CreateSecurityGroup: creates a security group without an idempotency token",
    ),
    decision(
        "ec2",
        "CreateSubnet",
        Decision::NotIdempotent,
        "CreateSubnet: each successful call can create a new subnet and the API has no client token",
    ),
    decision(
        "ec2",
        "CreateTags",
        Decision::Idempotent,
        "CreateTags: sets tag keys on named resources; repeating the same values has no additional effect",
    ),
    decision(
        "ec2",
        "CreateTransitGateway",
        Decision::NotIdempotent,
        "CreateTransitGateway: each successful call creates a new transit gateway and the API has no client token",
    ),
    decision(
        "ec2",
        "CreateTransitGatewayVpcAttachment",
        Decision::NotIdempotent,
        "CreateTransitGatewayVpcAttachment: creates an attachment without an idempotency token",
    ),
    decision(
        "ec2",
        "CreateVpc",
        Decision::NotIdempotent,
        "CreateVpc: each successful call creates a new VPC and the API has no client token",
    ),
    decision(
        "ec2",
        "CreateVpcEndpoint",
        Decision::Token("ClientToken"),
        "CreateVpcEndpoint: ClientToken identifies one endpoint creation request",
    ),
    decision(
        "ec2",
        "CreateVpcPeeringConnection",
        Decision::NotIdempotent,
        "CreateVpcPeeringConnection: creates a peering connection without an idempotency token",
    ),
    decision(
        "ec2",
        "CreateVpnGateway",
        Decision::NotIdempotent,
        "CreateVpnGateway: each successful call creates a new gateway and the API has no client token",
    ),
    decision(
        "ec2",
        "DeleteEgressOnlyInternetGateway",
        Decision::NotIdempotent,
        "DeleteEgressOnlyInternetGateway: the provider does not accept an already-absent gateway as success",
    ),
    decision(
        "ec2",
        "DeleteFlowLogs",
        Decision::NotIdempotent,
        "DeleteFlowLogs: the provider reports unsuccessful already-absent items as errors",
    ),
    decision(
        "ec2",
        "DeleteInternetGateway",
        Decision::NotIdempotent,
        "DeleteInternetGateway: the provider does not accept an already-absent gateway as success",
    ),
    decision(
        "ec2",
        "DeleteNatGateway",
        Decision::NotIdempotent,
        "DeleteNatGateway: the provider does not accept an already-absent NAT gateway as success",
    ),
    decision(
        "ec2",
        "DeleteRoute",
        Decision::NotIdempotent,
        "DeleteRoute: the provider does not accept an already-absent route as success",
    ),
    decision(
        "ec2",
        "DeleteRouteTable",
        Decision::NotIdempotent,
        "DeleteRouteTable: the provider does not accept an already-absent route table as success",
    ),
    decision(
        "ec2",
        "DeleteSecurityGroup",
        Decision::NotIdempotent,
        "DeleteSecurityGroup: the provider does not accept an already-absent security group as success",
    ),
    decision(
        "ec2",
        "DeleteSubnet",
        Decision::NotIdempotent,
        "DeleteSubnet: the provider does not accept an already-absent subnet as success",
    ),
    decision(
        "ec2",
        "DeleteTags",
        Decision::Idempotent,
        "DeleteTags: removes named tag keys; repeating the removal leaves the same state",
    ),
    decision(
        "ec2",
        "DeleteTransitGateway",
        Decision::NotIdempotent,
        "DeleteTransitGateway: the provider does not accept an already-deleting or absent transit gateway as success",
    ),
    decision(
        "ec2",
        "DeleteTransitGatewayVpcAttachment",
        Decision::NotIdempotent,
        "DeleteTransitGatewayVpcAttachment: the provider does not accept an already-deleting or absent attachment as success",
    ),
    decision(
        "ec2",
        "DeleteVpc",
        Decision::NotIdempotent,
        "DeleteVpc: the provider does not accept an already-absent VPC as success",
    ),
    decision(
        "ec2",
        "DeleteVpcEndpoints",
        Decision::NotIdempotent,
        "DeleteVpcEndpoints: the provider reports unsuccessful already-absent items as errors",
    ),
    decision(
        "ec2",
        "DeleteVpcPeeringConnection",
        Decision::NotIdempotent,
        "DeleteVpcPeeringConnection: the provider does not accept an already-absent connection as success",
    ),
    decision(
        "ec2",
        "DeleteVpnGateway",
        Decision::NotIdempotent,
        "DeleteVpnGateway: the provider does not accept an already-absent gateway as success",
    ),
    decision(
        "ec2",
        "DescribeAddresses",
        Decision::ReadOnly,
        "DescribeAddresses: reads address state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeEgressOnlyInternetGateways",
        Decision::ReadOnly,
        "DescribeEgressOnlyInternetGateways: reads gateway state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeFlowLogs",
        Decision::ReadOnly,
        "DescribeFlowLogs: reads flow-log state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeInternetGateways",
        Decision::ReadOnly,
        "DescribeInternetGateways: reads gateway state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeNatGateways",
        Decision::ReadOnly,
        "DescribeNatGateways: reads NAT-gateway state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeRouteTables",
        Decision::ReadOnly,
        "DescribeRouteTables: reads route-table state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeSecurityGroupRules",
        Decision::ReadOnly,
        "DescribeSecurityGroupRules: reads security-group rules and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeSecurityGroups",
        Decision::ReadOnly,
        "DescribeSecurityGroups: reads security-group state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeSubnets",
        Decision::ReadOnly,
        "DescribeSubnets: reads subnet state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeTransitGatewayVpcAttachments",
        Decision::ReadOnly,
        "DescribeTransitGatewayVpcAttachments: reads attachment state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeTransitGateways",
        Decision::ReadOnly,
        "DescribeTransitGateways: reads transit-gateway state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeVpcAttribute",
        Decision::ReadOnly,
        "DescribeVpcAttribute: reads one VPC attribute and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeVpcEndpoints",
        Decision::ReadOnly,
        "DescribeVpcEndpoints: reads endpoint state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeVpcPeeringConnections",
        Decision::ReadOnly,
        "DescribeVpcPeeringConnections: reads peering state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeVpcs",
        Decision::ReadOnly,
        "DescribeVpcs: reads VPC state and has no side effects",
    ),
    decision(
        "ec2",
        "DescribeVpnGateways",
        Decision::ReadOnly,
        "DescribeVpnGateways: reads VPN-gateway state and has no side effects",
    ),
    decision(
        "ec2",
        "DetachInternetGateway",
        Decision::NotIdempotent,
        "DetachInternetGateway: detaches a resource and the provider does not accept an already-detached result as success",
    ),
    decision(
        "ec2",
        "DetachVpnGateway",
        Decision::NotIdempotent,
        "DetachVpnGateway: detaches a resource and the provider does not accept an already-detached result as success",
    ),
    decision(
        "ec2",
        "DisassociateRouteTable",
        Decision::NotIdempotent,
        "DisassociateRouteTable: removes an association and the provider does not accept an already-removed result as success",
    ),
    decision(
        "ec2",
        "ModifySubnetAttribute",
        Decision::Idempotent,
        "ModifySubnetAttribute: sets one named subnet attribute to a complete value",
    ),
    decision(
        "ec2",
        "ModifyVpcAttribute",
        Decision::Idempotent,
        "ModifyVpcAttribute: sets one named VPC attribute to a complete value",
    ),
    decision(
        "ec2",
        "ModifyVpcEndpoint",
        Decision::NotIdempotent,
        "ModifyVpcEndpoint: applies incremental add/remove sets and a replay is not guaranteed to be accepted",
    ),
    decision(
        "ec2",
        "ReleaseAddress",
        Decision::NotIdempotent,
        "ReleaseAddress: the provider does not accept an already-released address as success",
    ),
    decision(
        "ec2",
        "ReplaceRoute",
        Decision::Idempotent,
        "ReplaceRoute: replaces the complete route target keyed by route table and destination",
    ),
    decision(
        "ec2",
        "ReplaceRouteTableAssociation",
        Decision::NotIdempotent,
        "ReplaceRouteTableAssociation: a successful call replaces the association identifier consumed by the request",
    ),
    decision(
        "ec2",
        "RevokeSecurityGroupEgress",
        Decision::NotIdempotent,
        "RevokeSecurityGroupEgress: revokes rules and the provider does not accept already-absent rules as success",
    ),
    decision(
        "ec2",
        "RevokeSecurityGroupIngress",
        Decision::NotIdempotent,
        "RevokeSecurityGroupIngress: revokes rules and the provider does not accept already-absent rules as success",
    ),
    decision(
        "iam",
        "CreateRole",
        Decision::NotIdempotent,
        "CreateRole: creates a top-level resource without an idempotency token",
    ),
    decision(
        "iam",
        "DeleteRole",
        Decision::NotIdempotent,
        "DeleteRole: the provider does not accept an already-absent role as success",
    ),
    decision(
        "iam",
        "GetRole",
        Decision::ReadOnly,
        "GetRole: reads role state and has no side effects",
    ),
    decision(
        "iam",
        "ListRoles",
        Decision::ReadOnly,
        "ListRoles: reads role state and has no side effects",
    ),
    decision(
        "iam",
        "TagRole",
        Decision::Idempotent,
        "TagRole: sets tag keys on one role; repeating the same values has no additional effect",
    ),
    decision(
        "iam",
        "UntagRole",
        Decision::Idempotent,
        "UntagRole: removes named tag keys; repeating the removal leaves the same state",
    ),
    decision(
        "iam",
        "UpdateAssumeRolePolicy",
        Decision::Idempotent,
        "UpdateAssumeRolePolicy: replaces the whole trust policy keyed by role name",
    ),
    decision(
        "iam",
        "UpdateRole",
        Decision::Idempotent,
        "UpdateRole: sets complete mutable role attributes keyed by role name",
    ),
    decision(
        "identitystore",
        "DescribeUser",
        Decision::ReadOnly,
        "DescribeUser: the model marks this read operation @readonly",
    ),
    decision(
        "identitystore",
        "GetUserId",
        Decision::ReadOnly,
        "GetUserId: the model marks this read operation @readonly",
    ),
    decision(
        "organizations",
        "CloseAccount",
        Decision::NotIdempotent,
        "CloseAccount: starts a top-level account transition and has no idempotency token",
    ),
    decision(
        "organizations",
        "CreateAccount",
        Decision::NotIdempotent,
        "CreateAccount: starts an account creation request without an idempotency token",
    ),
    decision(
        "organizations",
        "CreateOrganization",
        Decision::NotIdempotent,
        "CreateOrganization: creates a top-level organization without an idempotency token",
    ),
    decision(
        "organizations",
        "DeleteOrganization",
        Decision::NotIdempotent,
        "DeleteOrganization: the provider does not accept an already-absent organization as success",
    ),
    decision(
        "organizations",
        "DescribeAccount",
        Decision::ReadOnly,
        "DescribeAccount: reads account state and has no side effects",
    ),
    decision(
        "organizations",
        "DescribeCreateAccountStatus",
        Decision::ReadOnly,
        "DescribeCreateAccountStatus: reads request state and has no side effects",
    ),
    decision(
        "organizations",
        "DescribeOrganization",
        Decision::ReadOnly,
        "DescribeOrganization: reads organization state and has no side effects",
    ),
    decision(
        "organizations",
        "ListParents",
        Decision::ReadOnly,
        "ListParents: reads account parents and has no side effects",
    ),
    decision(
        "organizations",
        "ListTagsForResource",
        Decision::ReadOnly,
        "ListTagsForResource: reads resource tags and has no side effects",
    ),
    decision(
        "organizations",
        "MoveAccount",
        Decision::NotIdempotent,
        "MoveAccount: consumes the source parent relationship and a replay is not guaranteed to be accepted",
    ),
    decision(
        "organizations",
        "TagResource",
        Decision::Idempotent,
        "TagResource: sets tag keys on one resource; repeating the same values has no additional effect",
    ),
    decision(
        "organizations",
        "UntagResource",
        Decision::Idempotent,
        "UntagResource: removes named tag keys; repeating the removal leaves the same state",
    ),
    decision(
        "route53",
        "ChangeResourceRecordSets",
        Decision::NotIdempotent,
        "ChangeResourceRecordSets: applies a change batch without an idempotency token",
    ),
    decision(
        "route53",
        "ListResourceRecordSets",
        Decision::ReadOnly,
        "ListResourceRecordSets: reads record-set state and has no side effects",
    ),
    decision(
        "route53",
        "ListTagsForResource",
        Decision::ReadOnly,
        "ListTagsForResource: reads hosted-zone tags and has no side effects",
    ),
    decision(
        "s3",
        "CreateBucket",
        Decision::NotIdempotent,
        "CreateBucket: creates a top-level bucket without an idempotency token",
    ),
    decision(
        "s3",
        "DeleteBucket",
        Decision::NotIdempotent,
        "DeleteBucket: the provider does not accept an already-absent bucket as success",
    ),
    decision(
        "s3",
        "DeleteBucketCors",
        Decision::Idempotent,
        "DeleteBucketCors: removes the whole CORS sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteBucketEncryption",
        Decision::Idempotent,
        "DeleteBucketEncryption: removes the whole encryption sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteBucketLifecycle",
        Decision::Idempotent,
        "DeleteBucketLifecycle: removes the whole lifecycle sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteBucketOwnershipControls",
        Decision::Idempotent,
        "DeleteBucketOwnershipControls: removes the whole ownership-controls sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteBucketPolicy",
        Decision::Idempotent,
        "DeleteBucketPolicy: removes the whole policy sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteBucketReplication",
        Decision::Idempotent,
        "DeleteBucketReplication: removes the whole replication sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteBucketTagging",
        Decision::Idempotent,
        "DeleteBucketTagging: removes the whole tag set keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteBucketWebsite",
        Decision::Idempotent,
        "DeleteBucketWebsite: removes the whole website sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "DeleteObjects",
        Decision::Idempotent,
        "DeleteObjects: deletes the explicitly named object versions; deleting an absent key leaves it absent",
    ),
    decision(
        "s3",
        "DeletePublicAccessBlock",
        Decision::Idempotent,
        "DeletePublicAccessBlock: removes the whole public-access-block sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "GetBucketAcl",
        Decision::ReadOnly,
        "GetBucketAcl: reads bucket ACL state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketCors",
        Decision::ReadOnly,
        "GetBucketCors: reads bucket CORS state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketEncryption",
        Decision::ReadOnly,
        "GetBucketEncryption: reads bucket encryption state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketLifecycleConfiguration",
        Decision::ReadOnly,
        "GetBucketLifecycleConfiguration: reads bucket lifecycle state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketLogging",
        Decision::ReadOnly,
        "GetBucketLogging: reads bucket logging state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketNotificationConfiguration",
        Decision::ReadOnly,
        "GetBucketNotificationConfiguration: reads bucket notification state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketOwnershipControls",
        Decision::ReadOnly,
        "GetBucketOwnershipControls: reads bucket ownership-control state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketPolicy",
        Decision::ReadOnly,
        "GetBucketPolicy: reads bucket policy state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketReplication",
        Decision::ReadOnly,
        "GetBucketReplication: reads bucket replication state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketTagging",
        Decision::ReadOnly,
        "GetBucketTagging: reads bucket tags and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketVersioning",
        Decision::ReadOnly,
        "GetBucketVersioning: reads bucket versioning state and has no side effects",
    ),
    decision(
        "s3",
        "GetBucketWebsite",
        Decision::ReadOnly,
        "GetBucketWebsite: reads bucket website state and has no side effects",
    ),
    decision(
        "s3",
        "GetObjectLockConfiguration",
        Decision::ReadOnly,
        "GetObjectLockConfiguration: reads object-lock state and has no side effects",
    ),
    decision(
        "s3",
        "GetPublicAccessBlock",
        Decision::ReadOnly,
        "GetPublicAccessBlock: reads public-access-block state and has no side effects",
    ),
    decision(
        "s3",
        "HeadBucket",
        Decision::ReadOnly,
        "HeadBucket: reads bucket existence and region metadata and has no side effects",
    ),
    decision(
        "s3",
        "ListObjectVersions",
        Decision::ReadOnly,
        "ListObjectVersions: reads object-version state and has no side effects",
    ),
    decision(
        "s3",
        "PutBucketAcl",
        Decision::Idempotent,
        "PutBucketAcl: replaces the whole ACL keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketCors",
        Decision::Idempotent,
        "PutBucketCors: replaces the whole CORS sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketEncryption",
        Decision::Idempotent,
        "PutBucketEncryption: replaces the whole encryption sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketLifecycleConfiguration",
        Decision::Idempotent,
        "PutBucketLifecycleConfiguration: replaces the whole lifecycle sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketLogging",
        Decision::Idempotent,
        "PutBucketLogging: replaces the whole logging sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketNotificationConfiguration",
        Decision::Idempotent,
        "PutBucketNotificationConfiguration: replaces the whole notification sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketOwnershipControls",
        Decision::Idempotent,
        "PutBucketOwnershipControls: replaces the whole ownership-controls sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketPolicy",
        Decision::Idempotent,
        "PutBucketPolicy: replaces the whole policy keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketReplication",
        Decision::Idempotent,
        "PutBucketReplication: replaces the whole replication sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketTagging",
        Decision::Idempotent,
        "PutBucketTagging: replaces the whole tag set keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketVersioning",
        Decision::Idempotent,
        "PutBucketVersioning: sets the complete versioning state keyed by bucket",
    ),
    decision(
        "s3",
        "PutBucketWebsite",
        Decision::Idempotent,
        "PutBucketWebsite: replaces the whole website sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutObjectLockConfiguration",
        Decision::Idempotent,
        "PutObjectLockConfiguration: replaces the whole object-lock sub-configuration keyed by bucket",
    ),
    decision(
        "s3",
        "PutPublicAccessBlock",
        Decision::Idempotent,
        "PutPublicAccessBlock: replaces the whole public-access-block sub-configuration keyed by bucket",
    ),
    decision(
        "sqs",
        "CreateQueue",
        Decision::NotIdempotent,
        "CreateQueue: creates a top-level queue without an idempotency token",
    ),
    decision(
        "sqs",
        "DeleteQueue",
        Decision::NotIdempotent,
        "DeleteQueue: the provider does not accept an already-absent queue as success",
    ),
    decision(
        "sqs",
        "GetQueueAttributes",
        Decision::ReadOnly,
        "GetQueueAttributes: reads queue state and has no side effects",
    ),
    decision(
        "sqs",
        "ListQueueTags",
        Decision::ReadOnly,
        "ListQueueTags: reads queue tags and has no side effects",
    ),
    decision(
        "sqs",
        "SetQueueAttributes",
        Decision::Idempotent,
        "SetQueueAttributes: sets complete values for named queue attributes",
    ),
    decision(
        "sqs",
        "TagQueue",
        Decision::Idempotent,
        "TagQueue: sets tag keys on one queue; repeating the same values has no additional effect",
    ),
    decision(
        "sqs",
        "UntagQueue",
        Decision::Idempotent,
        "UntagQueue: removes named tag keys; repeating the removal leaves the same state",
    ),
    decision(
        "sts",
        "GetCallerIdentity",
        Decision::ReadOnly,
        "GetCallerIdentity: reads caller identity and has no side effects",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelDecision {
    ReadOnly,
    Idempotent,
    Token(String),
    NotIdempotent,
}

fn model_for_service<'a>(
    models: &'a HashMap<&str, SmithyModel>,
    service: &str,
) -> Option<&'a SmithyModel> {
    models
        .get(service)
        .or_else(|| models.get(format!("com.amazonaws.{service}").as_str()))
}

fn operation_id<'a>(model: &'a SmithyModel, operation: &str) -> Option<&'a str> {
    model
        .shapes
        .iter()
        .filter_map(|(id, shape)| match shape {
            Shape::Operation(_) => Some(id.as_str()),
            Shape::Service(_)
            | Shape::Structure(_)
            | Shape::Union(_)
            | Shape::Enum(_)
            | Shape::IntEnum(_)
            | Shape::List(_)
            | Shape::Map(_)
            | Shape::String(_)
            | Shape::Boolean(_)
            | Shape::Integer(_)
            | Shape::Long(_)
            | Shape::Float(_)
            | Shape::Double(_)
            | Shape::Blob(_)
            | Shape::Timestamp(_)
            | Shape::Document(_)
            | Shape::Resource(_) => None,
        })
        .find(|id| SmithyModel::shape_name(id) == operation)
}

fn model_decision(model: &SmithyModel, operation_id: &str) -> Result<ModelDecision> {
    let operation = model
        .get_operation(operation_id)
        .with_context(|| format!("{operation_id} is not an operation shape"))?;
    if operation.traits.contains_key(TRAIT_READONLY) {
        return Ok(ModelDecision::ReadOnly);
    }
    if operation.traits.contains_key(TRAIT_IDEMPOTENT) {
        return Ok(ModelDecision::Idempotent);
    }

    let token_members = operation
        .input
        .as_ref()
        .and_then(|input| model.get_structure(&input.target))
        .into_iter()
        .flat_map(|input| &input.members)
        .filter(|(_, member)| member.traits.contains_key(TRAIT_IDEMPOTENCY_TOKEN))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    match token_members.as_slice() {
        [] => Ok(ModelDecision::NotIdempotent),
        [member] => Ok(ModelDecision::Token(member.clone())),
        members => bail!(
            "{operation_id} has multiple @idempotencyToken members: {}",
            members.join(", ")
        ),
    }
}

pub fn validate_overrides(
    models: &HashMap<&str, SmithyModel>,
    overrides: &[Override],
) -> Result<()> {
    let mut seen = HashSet::new();
    for entry in overrides {
        let qualified = format!("{}.{}", entry.service, entry.operation);
        if !seen.insert((entry.service, entry.operation)) {
            bail!("duplicate resend-safety decision for {qualified}");
        }
        if entry.justification.trim().is_empty() {
            bail!("resend-safety decision for {qualified} has no justification");
        }

        let model = model_for_service(models, entry.service)
            .with_context(|| format!("service model for {} does not exist", entry.service))?;
        let operation_id = operation_id(model, entry.operation)
            .with_context(|| format!("operation {qualified} does not exist in the Smithy model"))?;
        let operation = model
            .get_operation(operation_id)
            .expect("operation_id only returns operation shapes");

        if let Decision::Token(member_name) = entry.decision {
            let input = operation
                .input
                .as_ref()
                .and_then(|input| model.get_structure(&input.target))
                .with_context(|| {
                    format!(
                        "token decision for {qualified} names input member {member_name}, but the operation has no structure input"
                    )
                })?;
            if !input.members.contains_key(member_name) {
                bail!(
                    "token decision for {qualified} names input member {member_name}, but that input member does not exist"
                );
            }
        }

        let modeled = model_decision(model, operation_id)?;
        let contradiction = match (&modeled, entry.decision) {
            (ModelDecision::ReadOnly, Decision::ReadOnly)
            | (ModelDecision::Idempotent, Decision::Idempotent)
            | (ModelDecision::NotIdempotent, _)
            | (ModelDecision::Token(_), Decision::Token(_)) => None,
            (ModelDecision::ReadOnly, _) => Some("@readonly"),
            (ModelDecision::Idempotent, _) => Some("@idempotent"),
            (ModelDecision::Token(_), _) => Some("@idempotencyToken"),
        };
        if let Some(trait_name) = contradiction {
            bail!(
                "resend-safety decision for {qualified} contradicts the model's {trait_name} trait"
            );
        }
        if let (ModelDecision::Token(modeled), Decision::Token(overridden)) =
            (&modeled, entry.decision)
            && modeled != overridden
        {
            bail!(
                "resend-safety decision for {qualified} names token member {overridden}, but the model's @idempotencyToken member is {modeled}"
            );
        }
    }
    Ok(())
}

pub fn generate_provider_module(
    models: &HashMap<&str, SmithyModel>,
    overrides: &[Override],
) -> Result<String> {
    validate_overrides(models, overrides)?;

    let mut services = overrides
        .iter()
        .map(|entry| entry.service)
        .collect::<Vec<_>>();
    services.sort_unstable();
    services.dedup();

    let mut code = String::from(
        "// @generated by carina-codegen-aws. Do not edit by hand.\n\
         #![allow(deprecated)]\n\n\
         use std::future::Future;\n\n\
         use aws_smithy_runtime_api::client::orchestrator::HttpResponse;\n\
         use aws_smithy_runtime_api::client::result::SdkError;\n\
         use carina_aws_types::Idempotency;\n\n\
         use crate::aws_operation::{AwsOperation, ClientToken, DecidedOperationError, DecidedSend, NoToken, TokenBearing, TokenOperation, WithToken};\n\
         use crate::helpers::TokenSendPermit;\n\
         use crate::resend_guard::ResendGuard;\n\
         use crate::sdk_config::BoundedSdkConfig;\n\n",
    );

    for service_name in &services {
        let model = model_for_service(models, service_name)
            .expect("validate_overrides verified every decided service model");
        let (service_shape_id, service_shape) = model.find_service().with_context(|| {
            format!("Smithy model for service {service_name} has no service shape")
        })?;
        let mut operations = overrides
            .iter()
            .filter(|entry| entry.service == *service_name)
            .collect::<Vec<_>>();
        operations.sort_by_key(|entry| entry.operation);

        let client_name = client_type_name(service_shape_id, service_shape);
        code.push_str(&format!(
            "/// Guarded AWS service client with a closed, generated operation API.\n\
             ///\n\
             /// Methods exist only for operations decided in\n\
             /// `carina-codegen-aws/src/resend_safety.rs` (`OVERRIDES`). To call a new AWS\n\
             /// operation, add a decision there and run `scripts/generate-provider.sh`.\n\
             /// Token-bearing operations return `TokenOperation` and must be sent via\n\
             /// `retry_aws_operation_with_token` or `send_aws_operation_with_token`.\n\
             pub(crate) struct {client_name} {{\n\
             \x20   inner: aws_sdk_{service_name}::Client,\n\
             }}\n\n\
             // Decisions can outlive their current call sites; every decided operation remains\n\
             // available through the closed generated client API.\n\
             #[allow(dead_code)]\n\
             impl {client_name} {{\n\
             \x20   // Raw SDK client construction is confined to generated guarded wrappers.\n\
             \x20   #[allow(clippy::disallowed_methods)]\n\
             \x20   pub(crate) fn from_bounded_config(config: &BoundedSdkConfig) -> Self {{\n\
             \x20       Self {{\n\
             \x20           inner: aws_sdk_{service_name}::Client::from_conf(\n\
             \x20               aws_sdk_{service_name}::config::Builder::from(config.sdk_config())\n\
             \x20                   .interceptor(ResendGuard::new({service_name}_idempotency))\n\
             \x20                   .build(),\n\
             \x20           ),\n\
             \x20       }}\n\
             \x20   }}\n\n\
             \x20   #[cfg(test)]\n\
             \x20   pub(crate) fn config(&self) -> &aws_sdk_{service_name}::Config {{\n\
             \x20       self.inner.config()\n\
             \x20   }}\n\n"
        ));
        for entry in &operations {
            let operation_method = entry.operation.to_snake_case();
            let fluent_builder = format!(
                "aws_sdk_{service_name}::operation::{operation_method}::builders::{}FluentBuilder",
                entry.operation
            );
            let operation = format!("self.inner.{operation_method}()");
            let (return_type, body) = match entry.decision {
                Decision::Token(_) => {
                    let input_builder = format!(
                        "aws_sdk_{service_name}::operation::{operation_method}::builders::{}InputBuilder",
                        entry.operation
                    );
                    (
                        format!("TokenOperation<{input_builder}, aws_sdk_{service_name}::Client>"),
                        format!(
                            "TokenOperation::new({input_builder}::default(), self.inner.clone())"
                        ),
                    )
                }
                Decision::ReadOnly | Decision::Idempotent | Decision::NotIdempotent => {
                    (fluent_builder, operation)
                }
            };
            code.push_str(&format!(
                "    /// {}\n\
                 \x20   pub(crate) fn {operation_method}(&self) -> {return_type} {{\n\
                 \x20       {body}\n\
                 \x20   }}\n\n",
                entry.justification
            ));
        }
        code.push_str("}\n\n");

        code.push_str(&format!(
            "pub(crate) fn {service_name}_idempotency(operation: &str) -> Idempotency {{\n    match operation {{\n"
        ));
        for entry in &operations {
            let operation_name = entry.operation;
            let idempotency = render_decision(entry.decision);
            code.push_str(&format!("        \"{operation_name}\" => {idempotency},\n"));
        }
        // Operation names are supplied by SDK metadata. A future or malformed
        // name must not accidentally acquire resend permission.
        code.push_str("        _ => Idempotency::NotIdempotent,\n    }\n}\n\n");
    }

    let mut ordered_overrides = overrides.iter().collect::<Vec<_>>();
    ordered_overrides.sort_by_key(|entry| (entry.service, entry.operation));
    for entry in &ordered_overrides {
        let operation_module = entry.operation.to_snake_case();
        let (builder, marker) = match entry.decision {
            Decision::Token(_) => (
                format!(
                    "aws_sdk_{}::operation::{operation_module}::builders::{}InputBuilder",
                    entry.service, entry.operation
                ),
                "WithToken",
            ),
            Decision::ReadOnly | Decision::Idempotent | Decision::NotIdempotent => (
                format!(
                    "aws_sdk_{}::operation::{operation_module}::builders::{}FluentBuilder",
                    entry.service, entry.operation
                ),
                "NoToken",
            ),
        };
        let idempotency = render_decision(entry.decision);

        code.push_str(&format!(
            "/// {}\n\
             impl AwsOperation for {builder} {{\n\
             \x20   type Resend = {marker};\n\
             \x20   type Output = aws_sdk_{}::operation::{operation_module}::{}Output;\n\
             \x20   type Error = aws_sdk_{}::operation::{operation_module}::{}Error;\n\n\
             \x20   const NAME: &'static str = \"{}.{}\";\n\
             \x20   const IDEMPOTENCY: Idempotency = {idempotency};\n\
             }}\n\n",
            entry.justification,
            entry.service,
            entry.operation,
            entry.service,
            entry.operation,
            entry.service,
            entry.operation,
        ));

        code.push_str(&format!(
            "impl DecidedOperationError for aws_sdk_{}::operation::{operation_module}::{}Error {{\n\
             \x20   const NAME: &'static str = <{builder} as AwsOperation>::NAME;\n\
             \x20   const IDEMPOTENCY: Idempotency = <{builder} as AwsOperation>::IDEMPOTENCY;\n\
             }}\n\n",
            entry.service, entry.operation,
        ));

        match entry.decision {
            Decision::Token(member) => {
                let setter = member.to_snake_case();
                code.push_str(&format!(
                    "impl TokenBearing for {builder} {{\n\
                     \x20   fn with_client_token(self, token: &ClientToken) -> Self {{\n\
                     \x20       self.{setter}(token.as_str())\n\
                     \x20   }}\n\
                     }}\n\n\
                     impl TokenOperation<{builder}, aws_sdk_{}::Client> {{\n\
                     \x20   // The permit proves the invocation token was applied before this network send.\n\
                     \x20   fn send_with_token(\n\
                     \x20       self,\n\
                     \x20       token: &ClientToken,\n\
                     \x20       permit: TokenSendPermit,\n\
                     \x20   ) -> impl Future<Output = Result<<Self as AwsOperation>::Output, SdkError<<Self as AwsOperation>::Error, HttpResponse>>> + Send {{\n\
                     \x20       self.send_with_token_using(token, permit, |input, client| async move {{\n\
                     \x20           input.send_with(&client).await\n\
                     \x20       }})\n\
                     \x20   }}\n\
                     }}\n\n\
                     impl<'a> DecidedSend<(&'a ClientToken, TokenSendPermit)> for TokenOperation<{builder}, aws_sdk_{}::Client> {{\n\
                     \x20   fn send(\n\
                     \x20       self,\n\
                     \x20       (token, permit): (&'a ClientToken, TokenSendPermit),\n\
                     \x20   ) -> impl Future<Output = Result<Self::Output, SdkError<Self::Error, HttpResponse>>> + Send {{\n\
                     \x20       self.send_with_token(token, permit)\n\
                     \x20   }}\n\
                     }}\n\n",
                    entry.service, entry.service,
                ));
            }
            Decision::ReadOnly | Decision::Idempotent | Decision::NotIdempotent => {
                code.push_str(&format!(
                    "impl DecidedSend<()> for {builder} {{\n\
                     \x20   fn send(\n\
                     \x20       self,\n\
                     \x20       (): (),\n\
                     \x20   ) -> impl Future<Output = Result<Self::Output, SdkError<Self::Error, HttpResponse>>> + Send {{\n\
                     \x20       self.send()\n\
                     \x20   }}\n\
                     }}\n\n"
                ));
            }
        }
    }

    Ok(code)
}

/// Generate the workspace Clippy policy from the same closed resend decision table.
pub fn generate_clippy_config(overrides: &[Override]) -> String {
    let mut services = overrides
        .iter()
        .map(|entry| entry.service)
        .collect::<Vec<_>>();
    services.sort_unstable();
    services.dedup();

    let mut code = String::from(
        "# @generated by carina-codegen-aws. Do not edit by hand.\n\n\
         disallowed-methods = [\n",
    );
    for service in services {
        for constructor in ["new", "from_conf"] {
            code.push_str(&format!(
                "    {{ path = \"aws_sdk_{service}::Client::{constructor}\", reason = \"use the generated service wrapper in carina-provider-aws/src/resend_safety_generated.rs\" }},\n"
            ));
        }
    }

    code.push_str("]\n");
    code
}

fn client_type_name(service_shape_id: &str, service_shape: &ServiceShape) -> String {
    let shape_name = SmithyModel::shape_name(service_shape_id);
    let sdk_id = service_shape
        .traits
        .get("aws.api#service")
        .and_then(|value| value.get("sdkId"))
        .and_then(|value| value.as_str())
        .unwrap_or(shape_name);

    // A collapsed lowercase SDK ID (for example, `identitystore`) loses the
    // modeled word boundary. Recover its original casing from the service
    // shape name without maintaining a per-service exception list.
    let modeled_name = if sdk_id
        .chars()
        .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        shape_name
            .to_ascii_lowercase()
            .find(sdk_id)
            .map(|start| &shape_name[start..start + sdk_id.len()])
            .unwrap_or(sdk_id)
    } else {
        sdk_id
    };
    format!("{}Client", modeled_name.to_upper_camel_case())
}

fn render_decision(decision: Decision) -> &'static str {
    match decision {
        Decision::ReadOnly => "Idempotency::ReadOnly",
        Decision::Idempotent => "Idempotency::Idempotent",
        Decision::Token(_) => "Idempotency::IdempotencyToken",
        Decision::NotIdempotent => "Idempotency::NotIdempotent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(operation: &str, operation_traits: &str, member: &str) -> SmithyModel {
        let member = if member.is_empty() {
            String::new()
        } else {
            format!(
                r#", "{member}": {{"target": "test#String", "traits": {{"smithy.api#idempotencyToken": {{}}}}}}"#
            )
        };
        carina_smithy::parse(&format!(
            r#"{{
                "smithy": "2.0",
                "shapes": {{
                    "test#Test": {{
                        "type": "service",
                        "version": "1",
                        "operations": [{{"target": "test#{operation}"}}]
                    }},
                    "test#{operation}": {{
                        "type": "operation",
                        "input": {{"target": "test#{operation}Input"}},
                        "traits": {{{operation_traits}}}
                    }},
                    "test#{operation}Input": {{
                        "type": "structure",
                        "members": {{"Name": {{"target": "test#String"}}{member}}}
                    }},
                    "test#String": {{"type": "string"}}
                }}
            }}"#
        ))
        .expect("valid test model")
    }

    fn models(model: SmithyModel) -> HashMap<&'static str, SmithyModel> {
        HashMap::from([("test", model)])
    }

    fn entry(operation: &'static str, decision: Decision) -> Override {
        Override {
            service: "test",
            operation,
            decision,
            justification: "test claim",
        }
    }

    #[test]
    fn unknown_operation_is_rejected() {
        let error = validate_overrides(
            &models(model("KnownOperation", "", "")),
            &[entry("MissingOperation", Decision::NotIdempotent)],
        )
        .expect_err("an override cannot name an absent operation");

        assert!(error.to_string().contains("test.MissingOperation"));
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn missing_token_member_is_rejected() {
        let error = validate_overrides(
            &models(model("CreateThing", "", "")),
            &[entry("CreateThing", Decision::Token("ClientToken"))],
        )
        .expect_err("a token decision must name an input member");

        assert!(error.to_string().contains("ClientToken"));
        assert!(error.to_string().contains("input member"));
    }

    #[test]
    fn contradiction_with_readonly_trait_is_rejected() {
        let error = validate_overrides(
            &models(model("GetThing", r#""smithy.api#readonly": {}"#, "")),
            &[entry("GetThing", Decision::NotIdempotent)],
        )
        .expect_err("an override cannot contradict @readonly");

        assert!(error.to_string().contains("test.GetThing"));
        assert!(error.to_string().contains("@readonly"));
    }

    #[test]
    fn contradiction_with_idempotency_token_trait_is_rejected() {
        let error = validate_overrides(
            &models(model("CreateThing", "", "ClientToken")),
            &[entry("CreateThing", Decision::NotIdempotent)],
        )
        .expect_err("an override cannot contradict @idempotencyToken");

        assert!(error.to_string().contains("test.CreateThing"));
        assert!(error.to_string().contains("@idempotencyToken"));
    }

    #[test]
    fn resource_bound_operation_is_included() {
        let model = carina_smithy::parse(
            r#"{
                "smithy": "2.0",
                "shapes": {
                    "test#Service": {
                        "type": "service",
                        "version": "1",
                        "operations": []
                    },
                    "test#DescribeThing": {
                        "type": "operation",
                        "traits": {"smithy.api#readonly": {}}
                    }
                }
            }"#,
        )
        .expect("valid model");
        let models = models(model);
        let overrides = [entry("DescribeThing", Decision::ReadOnly)];

        let generated = generate_provider_module(&models, &overrides)
            .expect("resource-bound operations are model operations too");

        assert!(generated.contains("\"DescribeThing\" => Idempotency::ReadOnly"));
    }

    #[test]
    fn generated_lookup_contains_only_decided_operations_and_default_deny() {
        let model = carina_smithy::parse(
            r#"{
                "smithy": "2.0",
                "shapes": {
                    "test#Service": {
                        "type": "service",
                        "version": "1",
                        "operations": [
                            {"target": "test#GetThing"},
                            {"target": "test#PutThing"},
                            {"target": "test#CreateThing"},
                            {"target": "test#OverrideThing"}
                        ]
                    },
                    "test#GetThing": {"type": "operation", "traits": {"smithy.api#readonly": {}}},
                    "test#PutThing": {"type": "operation", "traits": {"smithy.api#idempotent": {}}},
                    "test#CreateThing": {
                        "type": "operation",
                        "input": {"target": "test#CreateThingInput"}
                    },
                    "test#CreateThingInput": {
                        "type": "structure",
                        "members": {
                            "ClientToken": {
                                "target": "test#String",
                                "traits": {"smithy.api#idempotencyToken": {}}
                            }
                        }
                    },
                    "test#OverrideThing": {
                        "type": "operation",
                        "input": {"target": "test#OverrideThingInput"}
                    },
                    "test#OverrideThingInput": {
                        "type": "structure",
                        "members": {"Name": {"target": "test#String"}}
                    },
                    "test#String": {"type": "string"}
                }
            }"#,
        )
        .expect("valid model");
        let models = models(model);
        let overrides = [entry("OverrideThing", Decision::Idempotent)];

        let generated = generate_provider_module(&models, &overrides).expect("generation succeeds");

        assert!(
            generated.contains("pub(crate) fn test_idempotency(operation: &str) -> Idempotency")
        );
        assert!(!generated.contains("\"GetThing\" =>"));
        assert!(!generated.contains("\"PutThing\" =>"));
        assert!(!generated.contains("\"CreateThing\" =>"));
        assert!(generated.contains("\"OverrideThing\" => Idempotency::Idempotent"));
        assert!(generated.contains("_ => Idempotency::NotIdempotent"));
    }

    #[test]
    fn unrelated_model_operation_does_not_change_generated_output() {
        let without_unrelated = model("CreateThing", "", "ClientToken");
        let with_unrelated = carina_smithy::parse(
            r#"{
                "smithy": "2.0",
                "shapes": {
                    "test#Test": {
                        "type": "service",
                        "version": "1",
                        "operations": [
                            {"target": "test#CreateThing"},
                            {"target": "test#UnrelatedOperation"}
                        ]
                    },
                    "test#CreateThing": {
                        "type": "operation",
                        "input": {"target": "test#CreateThingInput"}
                    },
                    "test#CreateThingInput": {
                        "type": "structure",
                        "members": {
                            "Name": {"target": "test#String"},
                            "ClientToken": {
                                "target": "test#String",
                                "traits": {"smithy.api#idempotencyToken": {}}
                            }
                        }
                    },
                    "test#UnrelatedOperation": {
                        "type": "operation",
                        "traits": {"smithy.api#readonly": {}}
                    },
                    "test#String": {"type": "string"}
                }
            }"#,
        )
        .expect("valid model with an unrelated operation");
        let overrides = [entry("CreateThing", Decision::Token("ClientToken"))];

        let baseline = generate_provider_module(&models(without_unrelated), &overrides)
            .expect("baseline generation succeeds");
        let expanded = generate_provider_module(&models(with_unrelated), &overrides)
            .expect("generation with an unrelated operation succeeds");

        assert_eq!(expanded, baseline);
    }

    #[test]
    fn generated_impl_binds_input_builder_client_and_token_setter() {
        let model = model("CreateThing", "", "ClientToken");
        let models = models(model);
        let overrides = [entry("CreateThing", Decision::Token("ClientToken"))];

        let generated = generate_provider_module(&models, &overrides).expect("generation succeeds");

        assert!(generated.contains(
            "impl AwsOperation for aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder"
        ));
        assert!(generated.contains("type Resend = WithToken;"));
        assert!(
            generated.contains(
                "type Output = aws_sdk_test::operation::create_thing::CreateThingOutput;"
            )
        );
        assert!(
            generated
                .contains("type Error = aws_sdk_test::operation::create_thing::CreateThingError;")
        );
        assert!(generated.contains("const NAME: &'static str = \"test.CreateThing\";"));
        assert!(
            generated.contains("const IDEMPOTENCY: Idempotency = Idempotency::IdempotencyToken;")
        );
        let metadata_start = generated
            .find("impl AwsOperation for aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder")
            .expect("generated metadata impl");
        let metadata_end = generated[metadata_start..]
            .find("impl DecidedOperationError")
            .expect("generated error metadata impl");
        assert!(!generated[metadata_start..metadata_start + metadata_end].contains("fn send("));
        assert!(!generated.contains(
            "impl DecidedSend<()> for aws_sdk_test::operation::create_thing::builders::CreateThingFluentBuilder"
        ));
        assert!(generated.contains("impl<'a> DecidedSend<"));
        assert!(generated.contains(
            "for TokenOperation<aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder, aws_sdk_test::Client>"
        ));
        assert!(generated.contains("input.send_with(&client).await"));
        assert!(!generated.contains("CreateThingFluentBuilder"));
        assert!(!generated.contains("#![allow(clippy::disallowed_methods)]"));
        assert_eq!(
            generated
                .matches("#[allow(clippy::disallowed_methods)]")
                .count(),
            1
        );
        assert!(generated.contains(
            "impl DecidedOperationError for aws_sdk_test::operation::create_thing::CreateThingError"
        ));
        assert!(generated.contains(
            "const NAME: &'static str = <aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder as AwsOperation>::NAME;"
        ));
        assert!(generated.contains(
            "const IDEMPOTENCY: Idempotency = <aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder as AwsOperation>::IDEMPOTENCY;"
        ));
        assert!(generated.contains("impl TokenBearing for aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder"));
        assert!(generated.contains("self.client_token(token.as_str())"));
        assert!(generated.contains("pub(crate) struct TestClient"));
        assert!(generated.contains("inner: aws_sdk_test::Client"));
        assert!(generated.contains(
            "Methods exist only for operations decided in\n/// `carina-codegen-aws/src/resend_safety.rs` (`OVERRIDES`)."
        ));
        assert!(
            generated
                .contains("Token-bearing operations return `TokenOperation` and must be sent via")
        );
        assert!(
            generated
                .contains("pub(crate) fn from_bounded_config(config: &BoundedSdkConfig) -> Self")
        );
        assert!(generated.contains("aws_sdk_test::config::Builder::from(config.sdk_config())"));
        assert!(generated.contains(".interceptor(ResendGuard::new(test_idempotency))"));
        assert!(!generated.contains("pub(crate) fn from_conf("));
        assert!(generated.contains(
            "pub(crate) fn create_thing(&self) -> TokenOperation<aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder, aws_sdk_test::Client>"
        ));
        assert!(generated.contains(
            "TokenOperation::new(aws_sdk_test::operation::create_thing::builders::CreateThingInputBuilder::default(), self.inner.clone())"
        ));
        assert!(!generated.contains("DECIDED_OPERATIONS"));
        assert!(!generated.contains("TOKEN_OPERATIONS"));
    }

    #[test]
    fn generated_client_returns_raw_builder_only_for_decided_non_token_operation() {
        let model = model("GetThing", r#""smithy.api#readonly": {}"#, "");
        let models = models(model);
        let overrides = [entry("GetThing", Decision::ReadOnly)];

        let generated = generate_provider_module(&models, &overrides).expect("generation succeeds");

        assert!(generated.contains("pub(crate) struct TestClient"));
        assert!(generated.contains(
            "pub(crate) fn get_thing(&self) -> aws_sdk_test::operation::get_thing::builders::GetThingFluentBuilder"
        ));
        assert!(generated.contains("self.inner.get_thing()"));
        assert!(generated.contains(
            "impl DecidedSend<()> for aws_sdk_test::operation::get_thing::builders::GetThingFluentBuilder"
        ));
        assert!(!generated.contains("pub(crate) fn create_thing(&self)"));
    }

    #[test]
    fn generated_client_name_comes_from_smithy_service_shape() {
        let model = carina_smithy::parse(
            r#"{
                "smithy": "2.0",
                "shapes": {
                    "test#FancyService": {
                        "type": "service",
                        "version": "1",
                        "operations": [{"target": "test#GetThing"}]
                    },
                    "test#GetThing": {
                        "type": "operation",
                        "traits": {"smithy.api#readonly": {}}
                    }
                }
            }"#,
        )
        .expect("valid model");
        let models = models(model);
        let overrides = [entry("GetThing", Decision::ReadOnly)];

        let generated = generate_provider_module(&models, &overrides).expect("generation succeeds");

        assert!(generated.contains("pub(crate) struct FancyServiceClient"));
        assert!(!generated.contains("pub(crate) struct TestClient"));
    }

    #[test]
    fn generated_clippy_config_denies_only_raw_client_constructors() {
        let overrides = [
            entry("CreateThing", Decision::Token("ClientToken")),
            entry("GetThing", Decision::ReadOnly),
        ];

        let generated = generate_clippy_config(&overrides);

        assert!(generated.contains("disallowed-methods = ["));
        assert!(generated.contains("aws_sdk_test::Client::new"));
        assert!(generated.contains("aws_sdk_test::Client::from_conf"));
        assert_eq!(generated.matches("aws_sdk_test::Client::").count(), 2);
        assert!(!generated.contains("FluentBuilder::send"));
        assert!(!generated.contains("FluentBuilder::customize"));
    }
}
