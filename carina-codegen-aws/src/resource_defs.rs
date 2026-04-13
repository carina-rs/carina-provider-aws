//! Resource definitions for Smithy-based codegen.
//!
//! Each `ResourceDef` describes how to map AWS API operations to a Carina resource schema.
//! These definitions are consumed by the `smithy-codegen` binary.

/// An additional writable field not present in the create operation input.
/// Used to add fields from the read structure or synthetic fields.
pub struct ExtraField {
    /// PascalCase name for the generated attribute (e.g., "CidrIpv6", "SourcePrefixListId")
    pub name: &'static str,
    /// If Some, the field type is resolved from this read structure member.
    /// If None, type is inferred from the field name (e.g., resource ID patterns).
    pub read_source: Option<&'static str>,
    /// Manual description (used when read_source is None, or to override Smithy docs)
    pub description: Option<&'static str>,
}

/// A read operation that retrieves specific fields from an API response.
/// Used for resources that have no single "describe" structure (e.g., S3).
pub struct ReadOp {
    /// Operation short name (e.g., "GetBucketVersioning")
    pub operation: &'static str,
    /// Fields to extract: (smithy_output_field_name, optional_rename)
    pub fields: Vec<(&'static str, Option<&'static str>)>,
    /// Default values when the API returns None: (effective_field_name, default_value)
    pub defaults: Vec<(&'static str, &'static str)>,
}

/// Defines how to map an AWS API resource to a Carina schema.
pub struct ResourceDef {
    /// Carina DSL resource name (e.g., "ec2.vpc")
    pub name: &'static str,
    /// Smithy service namespace (e.g., "com.amazonaws.ec2")
    pub service_namespace: &'static str,
    /// Smithy structure to derive writable fields from, instead of the create op input.
    /// Use for APIs where resource fields are nested (e.g., Route 53 `ResourceRecordSet`
    /// inside `ChangeResourceRecordSets`). When None, fields come from `create_op` input.
    pub schema_structure: Option<&'static str>,
    /// Whether delete is a single API call (delete_op + identifier).
    /// true: VPC, Subnet, Route Table, Security Group, S3 Bucket
    /// false: IGW (detach+delete), Route (no-op), SG rules (multi-rule revoke)
    pub simple_delete: bool,
    /// Whether update is a no-op (just read back current state).
    /// true: Subnet, IGW, Route Table, Security Group
    /// false: VPC (DNS), S3 (versioning), Route (replace), SG rules (delete+recreate)
    pub noop_update: bool,
    /// Create operation short name (e.g., "CreateVpc")
    pub create_op: &'static str,
    /// Smithy structure name representing the read state (e.g., "Vpc").
    /// None for resources that use read_ops instead.
    pub read_structure: Option<&'static str>,
    /// Read operations for multi-operation resources (e.g., S3).
    /// When read_structure is None, fields are gathered from these operations.
    pub read_ops: Vec<ReadOp>,
    /// Delete operation short name (e.g., "DeleteVpc")
    pub delete_op: &'static str,
    /// Operations that modify existing resources
    pub update_ops: Vec<UpdateOp>,
    /// Primary identifier field name (e.g., "VpcId")
    pub identifier: &'static str,
    /// Whether this resource supports tags
    pub has_tags: bool,
    /// Type overrides: (field_name, type_code)
    pub type_overrides: Vec<(&'static str, &'static str)>,
    /// Fields to exclude from the schema
    pub exclude_fields: Vec<&'static str>,
    /// Fields to force as create-only even if they appear in update ops
    pub create_only_overrides: Vec<&'static str>,
    /// Enum aliases: (attr_snake_name, dsl_alias, canonical_value)
    pub enum_aliases: Vec<(&'static str, &'static str, &'static str)>,
    /// to_dsl overrides: (attr_snake_name, closure_code)
    pub to_dsl_overrides: Vec<(&'static str, &'static str)>,
    /// Required field overrides: fields that should be marked required
    /// even if not marked with smithy.api#required in the create input
    pub required_overrides: Vec<&'static str>,
    /// Extra read-only fields to include from the read structure
    /// that wouldn't normally be included (e.g., fields with different names)
    pub extra_read_only: Vec<&'static str>,
    /// Fields to force as read-only even if they appear in create input
    pub read_only_overrides: Vec<&'static str>,
    /// Extra writable fields to add as create-only attributes.
    /// These are fields not present in the create operation input.
    pub extra_writable: Vec<ExtraField>,
    /// Fields to mark as identity (contribute to anonymous resource identifier hashing).
    /// Use for attributes that distinguish same-type resources sharing create-only values.
    pub identity_overrides: Vec<&'static str>,
}

/// How fields are passed to an update API operation.
pub enum FieldLayout {
    /// Fields are top-level parameters of the API input.
    Flat(Vec<&'static str>),
    /// Fields are nested inside a named struct in the API input.
    InsideStruct {
        name: &'static str,
        fields: Vec<&'static str>,
    },
}

impl FieldLayout {
    /// Returns the field names regardless of layout.
    pub fn field_names(&self) -> &[&'static str] {
        match self {
            FieldLayout::Flat(fields) => fields,
            FieldLayout::InsideStruct { fields, .. } => fields,
        }
    }
}

/// An update operation and the fields it can modify.
pub struct UpdateOp {
    /// Operation short name (e.g., "ModifyVpcAttribute")
    pub operation: &'static str,
    /// How fields are passed to the API
    pub fields: FieldLayout,
}

/// Returns EC2 resource definitions.
pub fn ec2_resources() -> Vec<ResourceDef> {
    vec![
        // ec2.vpc
        ResourceDef {
            name: "ec2.vpc",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: false,
            create_op: "CreateVpc",
            read_structure: Some("Vpc"),
            read_ops: vec![],
            delete_op: "DeleteVpc",
            update_ops: vec![UpdateOp {
                operation: "ModifyVpcAttribute",
                fields: FieldLayout::Flat(vec!["EnableDnsHostnames", "EnableDnsSupport"]),
            }],
            identifier: "VpcId",
            has_tags: true,
            type_overrides: vec![("CidrBlock", "types::ipv4_cidr()")],
            exclude_fields: vec![
                "DryRun",
                "TagSpecifications",
                "AmazonProvidedIpv6CidrBlock",
                "Ipv6Pool",
                "Ipv6CidrBlock",
                "Ipv6IpamPoolId",
                "Ipv6CidrBlockNetworkBorderGroup",
                "Ipv6NetmaskLength",
                "VpcEncryptionControl",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.subnet
        ResourceDef {
            name: "ec2.subnet",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: false,
            create_op: "CreateSubnet",
            read_structure: Some("Subnet"),
            read_ops: vec![],
            delete_op: "DeleteSubnet",
            update_ops: vec![UpdateOp {
                operation: "ModifySubnetAttribute",
                fields: FieldLayout::Flat(vec![
                    "AssignIpv6AddressOnCreation",
                    "MapPublicIpOnLaunch",
                    "EnableDns64",
                    "EnableLniAtDeviceIndex",
                    "PrivateDnsNameOptionsOnLaunch",
                ]),
            }],
            identifier: "SubnetId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.internet_gateway
        ResourceDef {
            name: "ec2.internet_gateway",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: false,
            noop_update: true,
            create_op: "CreateInternetGateway",
            read_structure: Some("InternetGateway"),
            read_ops: vec![],
            delete_op: "DeleteInternetGateway",
            update_ops: vec![],
            identifier: "InternetGatewayId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.route_table
        ResourceDef {
            name: "ec2.route_table",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateRouteTable",
            read_structure: Some("RouteTable"),
            read_ops: vec![],
            delete_op: "DeleteRouteTable",
            update_ops: vec![],
            identifier: "RouteTableId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications", "ClientToken"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.route
        ResourceDef {
            name: "ec2.route",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "CreateRoute",
            read_structure: Some("Route"),
            read_ops: vec![],
            delete_op: "DeleteRoute",
            update_ops: vec![UpdateOp {
                operation: "ReplaceRoute",
                fields: FieldLayout::Flat(vec!["GatewayId", "NatGatewayId"]),
            }],
            identifier: "RouteTableId",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec![
                "DryRun",
                "OdbNetworkArn",
                "LocalTarget",
                "CarrierGatewayId",
                "CoreNetworkArn",
                "DestinationIpv6CidrBlock",
                "DestinationPrefixListId",
                "EgressOnlyInternetGatewayId",
                "InstanceId",
                "LocalGatewayId",
                "NetworkInterfaceId",
                "TransitGatewayId",
                "VpcEndpointId",
                "VpcPeeringConnectionId",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.security_group
        ResourceDef {
            name: "ec2.security_group",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateSecurityGroup",
            read_structure: Some("SecurityGroup"),
            read_ops: vec![],
            delete_op: "DeleteSecurityGroup",
            update_ops: vec![],
            identifier: "GroupId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.security_group_ingress
        ResourceDef {
            name: "ec2.security_group_ingress",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "AuthorizeSecurityGroupIngress",
            read_structure: Some("SecurityGroupRule"),
            read_ops: vec![],
            delete_op: "RevokeSecurityGroupIngress",
            update_ops: vec![],
            identifier: "SecurityGroupRuleId",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec![
                "DryRun",
                "TagSpecifications",
                "IpPermissions",
                "SecurityGroupRuleIds",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![("ip_protocol", "all", "-1")],
            to_dsl_overrides: vec![(
                "ip_protocol",
                r#"Some(|s: &str| match s { "-1" => "all".to_string(), _ => s.replace('-', "_") })"#,
            )],
            required_overrides: vec!["IpProtocol"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![
                ExtraField {
                    name: "CidrIpv6",
                    read_source: Some("CidrIpv6"),
                    description: None,
                },
                ExtraField {
                    name: "Description",
                    read_source: Some("Description"),
                    description: None,
                },
                ExtraField {
                    name: "SourcePrefixListId",
                    read_source: Some("PrefixListId"),
                    description: Some("The ID of the source prefix list."),
                },
                ExtraField {
                    name: "SourceSecurityGroupId",
                    read_source: None,
                    description: Some("The ID of the source security group."),
                },
            ],
            identity_overrides: vec![],
        },
        // ec2.security_group_egress
        ResourceDef {
            name: "ec2.security_group_egress",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "AuthorizeSecurityGroupEgress",
            read_structure: Some("SecurityGroupRule"),
            read_ops: vec![],
            delete_op: "RevokeSecurityGroupEgress",
            update_ops: vec![],
            identifier: "SecurityGroupRuleId",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec![
                "DryRun",
                "TagSpecifications",
                "IpPermissions",
                "SecurityGroupRuleIds",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![("ip_protocol", "all", "-1")],
            to_dsl_overrides: vec![(
                "ip_protocol",
                r#"Some(|s: &str| match s { "-1" => "all".to_string(), _ => s.replace('-', "_") })"#,
            )],
            required_overrides: vec!["IpProtocol", "GroupId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![
                ExtraField {
                    name: "CidrIpv6",
                    read_source: Some("CidrIpv6"),
                    description: None,
                },
                ExtraField {
                    name: "Description",
                    read_source: Some("Description"),
                    description: None,
                },
                ExtraField {
                    name: "DestinationPrefixListId",
                    read_source: Some("PrefixListId"),
                    description: Some("The ID of the destination prefix list."),
                },
                ExtraField {
                    name: "DestinationSecurityGroupId",
                    read_source: None,
                    description: Some("The ID of the destination security group."),
                },
            ],
            identity_overrides: vec![],
        },
        // ec2.egress_only_internet_gateway
        ResourceDef {
            name: "ec2.egress_only_internet_gateway",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateEgressOnlyInternetGateway",
            read_structure: Some("EgressOnlyInternetGateway"),
            read_ops: vec![],
            delete_op: "DeleteEgressOnlyInternetGateway",
            update_ops: vec![],
            identifier: "EgressOnlyInternetGatewayId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications", "ClientToken"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["VpcId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.eip
        ResourceDef {
            name: "ec2.eip",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "AllocateAddress",
            read_structure: Some("Address"),
            read_ops: vec![],
            delete_op: "ReleaseAddress",
            update_ops: vec![],
            identifier: "AllocationId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec![
                "DryRun",
                "TagSpecifications",
                "CustomerOwnedIpv4Pool",
                "IpamPoolId",
                "NetworkBorderGroup",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec!["PublicIp"],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.flow_log
        ResourceDef {
            name: "ec2.flow_log",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateFlowLogs",
            read_structure: Some("FlowLog"),
            read_ops: vec![],
            delete_op: "DeleteFlowLogs",
            update_ops: vec![],
            identifier: "FlowLogId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec![
                "DryRun",
                "TagSpecifications",
                "ClientToken",
                "DeliverCrossAccountRole",
                "DestinationOptions",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![(
                "log_destination_type",
                r#"Some(|s: &str| s.replace('-', "_"))"#,
            )],
            required_overrides: vec!["ResourceId", "ResourceType"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.nat_gateway
        ResourceDef {
            name: "ec2.nat_gateway",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateNatGateway",
            read_structure: Some("NatGateway"),
            read_ops: vec![],
            delete_op: "DeleteNatGateway",
            update_ops: vec![],
            identifier: "NatGatewayId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec![
                "DryRun",
                "TagSpecifications",
                "ClientToken",
                "SecondaryAllocationIds",
                "SecondaryPrivateIpAddresses",
                "SecondaryPrivateIpAddressCount",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["SubnetId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.subnet_route_table_association
        ResourceDef {
            name: "ec2.subnet_route_table_association",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "AssociateRouteTable",
            read_structure: Some("RouteTableAssociation"),
            read_ops: vec![],
            delete_op: "DisassociateRouteTable",
            update_ops: vec![],
            identifier: "AssociationId",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "GatewayId"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["RouteTableId", "SubnetId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.transit_gateway
        ResourceDef {
            name: "ec2.transit_gateway",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: false,
            create_op: "CreateTransitGateway",
            read_structure: Some("TransitGateway"),
            read_ops: vec![],
            delete_op: "DeleteTransitGateway",
            update_ops: vec![UpdateOp {
                operation: "ModifyTransitGateway",
                fields: FieldLayout::Flat(vec!["Description"]),
            }],
            identifier: "TransitGatewayId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.transit_gateway_attachment
        ResourceDef {
            name: "ec2.transit_gateway_attachment",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateTransitGatewayVpcAttachment",
            read_structure: Some("TransitGatewayVpcAttachment"),
            read_ops: vec![],
            delete_op: "DeleteTransitGatewayVpcAttachment",
            update_ops: vec![],
            identifier: "TransitGatewayAttachmentId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["TransitGatewayId", "VpcId", "SubnetIds"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.vpc_endpoint
        ResourceDef {
            name: "ec2.vpc_endpoint",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: false,
            create_op: "CreateVpcEndpoint",
            read_structure: Some("VpcEndpoint"),
            read_ops: vec![],
            delete_op: "DeleteVpcEndpoints",
            update_ops: vec![UpdateOp {
                operation: "ModifyVpcEndpoint",
                fields: FieldLayout::Flat(vec!["PolicyDocument", "PrivateDnsEnabled"]),
            }],
            identifier: "VpcEndpointId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec![
                "DryRun",
                "TagSpecifications",
                "ClientToken",
                "DnsOptions",
                "IpAddressType",
                "SubnetConfigurations",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["ServiceName", "VpcId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.vpc_gateway_attachment
        ResourceDef {
            name: "ec2.vpc_gateway_attachment",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: false,
            noop_update: true,
            create_op: "AttachInternetGateway",
            read_structure: None,
            read_ops: vec![],
            delete_op: "DetachInternetGateway",
            update_ops: vec![],
            identifier: "VpcId",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["VpcId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![
                ExtraField {
                    name: "InternetGatewayId",
                    read_source: None,
                    description: Some("The ID of the internet gateway."),
                },
                ExtraField {
                    name: "VpnGatewayId",
                    read_source: None,
                    description: Some("The ID of the VPN gateway."),
                },
            ],
            identity_overrides: vec![],
        },
        // ec2.vpc_peering_connection
        ResourceDef {
            name: "ec2.vpc_peering_connection",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateVpcPeeringConnection",
            read_structure: Some("VpcPeeringConnection"),
            read_ops: vec![],
            delete_op: "DeleteVpcPeeringConnection",
            update_ops: vec![],
            identifier: "VpcPeeringConnectionId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications", "PeerRegion"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["VpcId", "PeerVpcId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // ec2.vpn_gateway
        ResourceDef {
            name: "ec2.vpn_gateway",
            service_namespace: "com.amazonaws.ec2",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateVpnGateway",
            read_structure: Some("VpnGateway"),
            read_ops: vec![],
            delete_op: "DeleteVpnGateway",
            update_ops: vec![],
            identifier: "VpnGatewayId",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["DryRun", "TagSpecifications"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Type"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
    ]
}

/// Returns STS resource definitions (data sources).
pub fn sts_resources() -> Vec<ResourceDef> {
    vec![
        // sts.caller_identity (data source: no create/delete)
        ResourceDef {
            name: "sts.caller_identity",
            service_namespace: "com.amazonaws.sts",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "",
            read_structure: None,
            read_ops: vec![ReadOp {
                operation: "GetCallerIdentity",
                fields: vec![
                    ("Account", Some("AccountId")),
                    ("Arn", None),
                    ("UserId", None),
                ],
                defaults: vec![],
            }],
            delete_op: "",
            update_ops: vec![],
            identifier: "",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec![],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
    ]
}

/// Returns Organizations resource definitions.
pub fn organizations_resources() -> Vec<ResourceDef> {
    vec![
        // organizations.organization
        ResourceDef {
            name: "organizations.organization",
            service_namespace: "com.amazonaws.organizations",
            schema_structure: None,
            simple_delete: true,
            noop_update: true,
            create_op: "CreateOrganization",
            read_structure: Some("Organization"),
            read_ops: vec![],
            delete_op: "DeleteOrganization",
            update_ops: vec![],
            identifier: "Id",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec!["AvailablePolicyTypes"],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![
                "Arn",
                "MasterAccountArn",
                "MasterAccountId",
                "MasterAccountEmail",
            ],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
        // organizations.account
        ResourceDef {
            name: "organizations.account",
            service_namespace: "com.amazonaws.organizations",
            schema_structure: None,
            simple_delete: false,
            noop_update: true,
            create_op: "CreateAccount",
            read_structure: Some("Account"),
            read_ops: vec![],
            delete_op: "CloseAccount",
            update_ops: vec![],
            identifier: "Id",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec!["Paths", "State"],
            create_only_overrides: vec![
                "AccountName",
                "Email",
                "IamUserAccessToBilling",
                "RoleName",
            ],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["AccountName", "Email"],
            extra_read_only: vec!["Arn", "Name", "Status", "JoinedMethod", "JoinedTimestamp"],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
    ]
}

/// Returns S3 resource definitions.
pub fn s3_resources() -> Vec<ResourceDef> {
    vec![
        // s3.bucket
        ResourceDef {
            name: "s3.bucket",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false, // manually implemented to support lifecycle.force_delete
            noop_update: false,
            create_op: "CreateBucket",
            read_structure: None,
            read_ops: vec![ReadOp {
                operation: "GetBucketVersioning",
                fields: vec![("Status", Some("VersioningStatus"))],
                defaults: vec![("VersioningStatus", "Suspended")],
            }],
            delete_op: "DeleteBucket",
            update_ops: vec![
                UpdateOp {
                    operation: "PutBucketVersioning",
                    fields: FieldLayout::InsideStruct {
                        name: "VersioningConfiguration",
                        fields: vec!["VersioningStatus"],
                    },
                },
                UpdateOp {
                    operation: "PutBucketOwnershipControls",
                    fields: FieldLayout::Flat(vec!["ObjectOwnership"]),
                },
                UpdateOp {
                    operation: "PutBucketAcl",
                    fields: FieldLayout::Flat(vec![
                        "ACL",
                        "GrantFullControl",
                        "GrantRead",
                        "GrantReadACP",
                        "GrantWrite",
                        "GrantWriteACP",
                    ]),
                },
            ],
            identifier: "Bucket",
            has_tags: true,
            type_overrides: vec![],
            exclude_fields: vec![
                "CreateBucketConfiguration",
                "ContentMD5",
                "ChecksumAlgorithm",
                "MFA",
                "ExpectedBucketOwner",
                "VersioningConfiguration",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![],
            identity_overrides: vec![],
        },
    ]
}

/// Returns Route 53 resource definitions.
pub fn route53_resources() -> Vec<ResourceDef> {
    vec![
        // route53.record_set
        // Uses schema_structure because ChangeResourceRecordSets wraps fields
        // in a nested ChangeBatch, not as top-level input parameters.
        ResourceDef {
            name: "route53.record_set",
            service_namespace: "com.amazonaws.route53",
            schema_structure: Some("ResourceRecordSet"),
            simple_delete: false,
            noop_update: false,
            create_op: "ChangeResourceRecordSets",
            read_structure: Some("ResourceRecordSet"),
            read_ops: vec![],
            delete_op: "ChangeResourceRecordSets",
            update_ops: vec![],
            identifier: "Name",
            has_tags: false,
            type_overrides: vec![
                // Smithy has ResourceRecords as List<Struct{Value}>, but for DSL
                // simplicity we flatten to List<String> since each record is a
                // single value string.
                (
                    "ResourceRecords",
                    "AttributeType::list(AttributeType::String)",
                ),
            ],
            exclude_fields: vec![
                // Routing policy fields — out of scope for initial version
                "SetIdentifier",
                "Weight",
                "Region",
                "Failover",
                "MultiValueAnswer",
                "GeoLocation",
                "GeoProximityLocation",
                "HealthCheckId",
                "TrafficPolicyInstanceId",
                "CidrRoutingConfig",
            ],
            create_only_overrides: vec!["Name"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Name", "Type"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_writable: vec![ExtraField {
                name: "HostedZoneId",
                read_source: None,
                description: Some("The ID of the hosted zone that contains this record set."),
            }],
            identity_overrides: vec!["Type"],
        },
    ]
}
