//! Resource definitions for Smithy-based codegen.
//!
//! Each `ResourceDef` describes how to map AWS API operations to a Carina resource schema.
//! Each `DataSourceDef` describes a read-only data source with optional user-supplied lookup inputs.
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
    /// Carina DSL resource name (e.g., "ec2.Vpc")
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
    /// Read-back projections for attributes that are not direct members of
    /// the read structure. The DSL attribute already lives in the schema
    /// (typically because the create input already contains it); this only
    /// tells the extraction emitter how to recover the value from the
    /// response shape — see `DerivedSource`.
    pub derived_attributes: Vec<DerivedAttribute>,
}

/// One read-back projection for `extract_*_attributes`.
///
/// `attr` is the DSL-side attribute name in the same spelling used
/// elsewhere in `ResourceDef` (PascalCase or pre-snake_cased member name);
/// the emitter snake_cases it for the attribute key.
pub struct DerivedAttribute {
    pub attr: &'static str,
    pub source: DerivedSource,
}

/// Where the value for a `DerivedAttribute` lives in the SDK response.
///
/// Marked `#[non_exhaustive]` so adding a new projection in a future
/// sub-issue (B-3, B-4) does not silently force every external match arm
/// to compile. `carina-codegen-aws` is currently the only consumer.
#[non_exhaustive]
pub enum DerivedSource {
    /// `obj.<list_member>().first().and_then(|x| x.<child_member>())`.
    /// Used for SDK responses whose top-level field is a `Vec<Struct>` and
    /// the resource convention is to read only the first element
    /// (e.g. `NatGateway.NatGatewayAddresses[0].AllocationId`).
    ListFirst {
        list_member: &'static str,
        child_member: &'static str,
    },
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

/// A read-only data source definition.
///
/// Data sources have no create/update/delete lifecycle. They look up existing
/// AWS resources via user-supplied inputs and return their attributes.
/// Lookup logic is hand-written in `services/{service}/{resource}.rs`;
/// the codegen generates schema, docs, and dispatch boilerplate.
pub struct DataSourceDef {
    /// Carina DSL resource name (e.g., "identitystore.User")
    pub name: &'static str,
    /// Smithy service namespace (e.g., "com.amazonaws.identitystore")
    pub service_namespace: &'static str,
    /// User-supplied lookup input fields (empty for zero-input data sources)
    pub inputs: Vec<DataSourceInput>,
    /// Declared output attributes (read-only fields exposed by the data source).
    pub output_attributes: Vec<DataSourceOutput>,
    /// Read operations that retrieve output fields
    pub read_ops: Vec<ReadOp>,
    /// Type overrides: (field_name, type_code)
    pub type_overrides: Vec<(&'static str, &'static str)>,
    /// Fields to exclude from the schema
    pub exclude_fields: Vec<&'static str>,
}

/// A user-supplied input field for a data source lookup.
pub struct DataSourceInput {
    /// DSL field name (e.g., "user_name")
    pub name: &'static str,
    /// Smithy/AWS field name (e.g., "UserName")
    pub provider_name: &'static str,
    /// Human-readable description for docs
    pub description: &'static str,
    /// Whether this input is required
    pub required: bool,
    /// Type override (e.g., "AttributeType::String"). None = infer from Smithy.
    pub type_override: Option<&'static str>,
}

/// One declared output attribute on a `DataSourceDef`.
///
/// `provider_name = None` means the value is computed (e.g. ARN built from
/// inputs); `provider_name = Some("...")` means the value comes from a
/// `read_ops` API field of that name.
pub struct DataSourceOutput {
    /// DSL field name (e.g., "account_id")
    pub name: &'static str,
    /// Smithy/AWS field name (e.g., "Account"). None for computed outputs.
    pub provider_name: Option<&'static str>,
    /// Human-readable description for docs
    pub description: &'static str,
    /// Rust type expression for codegen, e.g. `"AttributeType::String"` or
    /// `"super::aws_account_id()"`. Required: codegen does not infer.
    pub type_code: &'static str,
}

/// Returns EC2 resource definitions.
pub fn ec2_resources() -> Vec<ResourceDef> {
    vec![
        // ec2.vpc
        ResourceDef {
            name: "ec2.Vpc",
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
            derived_attributes: vec![],
        },
        // ec2.subnet
        ResourceDef {
            name: "ec2.Subnet",
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
            derived_attributes: vec![],
        },
        // ec2.internet_gateway
        ResourceDef {
            name: "ec2.InternetGateway",
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
            extra_writable: vec![ExtraField {
                name: "VpcId",
                read_source: None,
                description: Some(
                    "The ID of the VPC to attach the internet gateway to. The provider attaches the IGW after creation and detaches before deletion.",
                ),
            }],
            identity_overrides: vec![],
            derived_attributes: vec![],
        },
        // ec2.route_table
        ResourceDef {
            name: "ec2.RouteTable",
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
            derived_attributes: vec![],
        },
        // ec2.route
        ResourceDef {
            name: "ec2.Route",
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
            derived_attributes: vec![],
        },
        // ec2.security_group
        ResourceDef {
            name: "ec2.SecurityGroup",
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
            derived_attributes: vec![],
        },
        // ec2.security_group_ingress
        ResourceDef {
            name: "ec2.SecurityGroupIngress",
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
            derived_attributes: vec![],
        },
        // ec2.security_group_egress
        ResourceDef {
            name: "ec2.SecurityGroupEgress",
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
            derived_attributes: vec![],
        },
        // ec2.egress_only_internet_gateway
        ResourceDef {
            name: "ec2.EgressOnlyInternetGateway",
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
            // VpcId lives on Attachments[0].VpcId in the read response;
            // there is no top-level VpcId getter on EgressOnlyInternetGateway.
            derived_attributes: vec![DerivedAttribute {
                attr: "VpcId",
                source: DerivedSource::ListFirst {
                    list_member: "Attachments",
                    child_member: "VpcId",
                },
            }],
        },
        // ec2.eip
        ResourceDef {
            name: "ec2.Eip",
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
            derived_attributes: vec![],
        },
        // ec2.flow_log
        ResourceDef {
            name: "ec2.FlowLog",
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
            derived_attributes: vec![],
        },
        // ec2.nat_gateway
        ResourceDef {
            name: "ec2.NatGateway",
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
            // AllocationId lives on NatGatewayAddresses[0].AllocationId in the
            // read response; there is no top-level AllocationId getter on
            // NatGateway.
            derived_attributes: vec![DerivedAttribute {
                attr: "AllocationId",
                source: DerivedSource::ListFirst {
                    list_member: "NatGatewayAddresses",
                    child_member: "AllocationId",
                },
            }],
        },
        // ec2.subnet_route_table_association
        ResourceDef {
            name: "ec2.SubnetRouteTableAssociation",
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
            derived_attributes: vec![],
        },
        // ec2.transit_gateway
        ResourceDef {
            name: "ec2.TransitGateway",
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
            derived_attributes: vec![],
        },
        // ec2.transit_gateway_attachment
        ResourceDef {
            name: "ec2.TransitGatewayAttachment",
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
            derived_attributes: vec![],
        },
        // ec2.vpc_endpoint
        ResourceDef {
            name: "ec2.VpcEndpoint",
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
            derived_attributes: vec![],
        },
        // ec2.vpc_gateway_attachment
        ResourceDef {
            name: "ec2.VpcGatewayAttachment",
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
            derived_attributes: vec![],
        },
        // ec2.vpc_peering_connection
        ResourceDef {
            name: "ec2.VpcPeeringConnection",
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
            derived_attributes: vec![],
        },
        // ec2.vpn_gateway
        ResourceDef {
            name: "ec2.VpnGateway",
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
            derived_attributes: vec![],
        },
    ]
}

/// Returns STS resource definitions (data sources).
pub fn sts_resources() -> Vec<ResourceDef> {
    vec![]
}

/// Returns STS data source definitions.
pub fn sts_data_sources() -> Vec<DataSourceDef> {
    vec![DataSourceDef {
        name: "sts.CallerIdentity",
        service_namespace: "com.amazonaws.sts",
        inputs: vec![],
        output_attributes: vec![
            DataSourceOutput {
                name: "account_id",
                provider_name: Some("Account"),
                description: "The Amazon Web Services account ID number of the account that owns or contains the calling entity.",
                type_code: "super::aws_account_id()",
            },
            DataSourceOutput {
                name: "arn",
                provider_name: Some("Arn"),
                description: "The Amazon Web Services ARN associated with the calling entity.",
                type_code: "super::arn()",
            },
            DataSourceOutput {
                name: "user_id",
                provider_name: Some("UserId"),
                description: "The unique identifier of the calling entity.",
                type_code: "AttributeType::String",
            },
        ],
        read_ops: vec![ReadOp {
            operation: "GetCallerIdentity",
            fields: vec![
                ("Account", Some("AccountId")),
                ("Arn", None),
                ("UserId", None),
            ],
            defaults: vec![],
        }],
        type_overrides: vec![
            ("AccountId", "super::aws_account_id()"),
            ("Arn", "super::arn()"),
        ],
        exclude_fields: vec![],
    }]
}

/// Returns Identity Store data source definitions.
pub fn identitystore_data_sources() -> Vec<DataSourceDef> {
    vec![DataSourceDef {
        name: "identitystore.User",
        service_namespace: "com.amazonaws.identitystore",
        inputs: vec![
            DataSourceInput {
                name: "identity_store_id",
                provider_name: "IdentityStoreId",
                description: "The globally unique identifier for the identity store.",
                required: true,
                type_override: None,
            },
            DataSourceInput {
                name: "user_id",
                provider_name: "UserId",
                description: "The identifier for the user. Provide either user_id or user_name.",
                required: false,
                type_override: None,
            },
            DataSourceInput {
                name: "user_name",
                provider_name: "UserName",
                description: "The user's user name. Provide either user_id or user_name.",
                required: false,
                type_override: None,
            },
        ],
        output_attributes: vec![
            DataSourceOutput {
                name: "display_name",
                provider_name: Some("DisplayName"),
                description: "Display name of the user.",
                type_code: "AttributeType::String",
            },
            DataSourceOutput {
                name: "emails",
                provider_name: Some("Emails"),
                description: "Email addresses associated with the user.",
                type_code: "AttributeType::String",
            },
        ],
        read_ops: vec![ReadOp {
            operation: "DescribeUser",
            fields: vec![("DisplayName", None), ("Emails", None)],
            defaults: vec![],
        }],
        type_overrides: vec![],
        exclude_fields: vec![],
    }]
}

/// Returns Organizations resource definitions.
pub fn organizations_resources() -> Vec<ResourceDef> {
    vec![
        // organizations.organization
        ResourceDef {
            name: "organizations.Organization",
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
            derived_attributes: vec![],
        },
        // organizations.account
        ResourceDef {
            name: "organizations.Account",
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
            derived_attributes: vec![],
        },
    ]
}

/// Returns S3 resource definitions.
pub fn s3_resources() -> Vec<ResourceDef> {
    vec![
        // s3.bucket
        ResourceDef {
            name: "s3.Bucket",
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
            derived_attributes: vec![],
        },
    ]
}

/// Returns S3 data source definitions.
pub fn s3_data_sources() -> Vec<DataSourceDef> {
    vec![DataSourceDef {
        name: "s3.Bucket",
        service_namespace: "com.amazonaws.s3",
        inputs: vec![DataSourceInput {
            name: "bucket",
            provider_name: "Bucket",
            description: "Name of the S3 bucket to look up.",
            required: true,
            type_override: None,
        }],
        output_attributes: vec![
            DataSourceOutput {
                name: "bucket",
                provider_name: None,
                description: "The bucket name (echo of the input).",
                type_code: "AttributeType::String",
            },
            DataSourceOutput {
                name: "arn",
                provider_name: None,
                description: "ARN of the bucket.",
                type_code: "super::arn()",
            },
            DataSourceOutput {
                name: "region",
                provider_name: Some("LocationConstraint"),
                description: "AWS region the bucket is in.",
                type_code: "AttributeType::String",
            },
            DataSourceOutput {
                name: "bucket_domain_name",
                provider_name: None,
                description: "Bucket domain name (`<bucket>.s3.amazonaws.com`).",
                type_code: "AttributeType::String",
            },
            DataSourceOutput {
                name: "bucket_regional_domain_name",
                provider_name: None,
                description: "Region-specific bucket domain name (`<bucket>.s3.<region>.amazonaws.com`).",
                type_code: "AttributeType::String",
            },
            DataSourceOutput {
                name: "hosted_zone_id",
                provider_name: None,
                description: "Route 53 Hosted Zone ID for the bucket's region.",
                type_code: "AttributeType::String",
            },
        ],
        read_ops: vec![
            ReadOp {
                operation: "HeadBucket",
                fields: vec![],
                defaults: vec![],
            },
            ReadOp {
                operation: "GetBucketLocation",
                fields: vec![("LocationConstraint", None)],
                defaults: vec![("LocationConstraint", "us-east-1")],
            },
        ],
        type_overrides: vec![],
        exclude_fields: vec![],
    }]
}

/// Returns Route 53 resource definitions.
pub fn route53_resources() -> Vec<ResourceDef> {
    vec![
        // route53.record_set
        // Uses schema_structure because ChangeResourceRecordSets wraps fields
        // in a nested ChangeBatch, not as top-level input parameters.
        ResourceDef {
            name: "route53.RecordSet",
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
            derived_attributes: vec![],
        },
    ]
}

/// Returns IAM resource definitions.
pub fn iam_resources() -> Vec<ResourceDef> {
    vec![ResourceDef {
        name: "iam.Role",
        service_namespace: "com.amazonaws.iam",
        schema_structure: None,
        simple_delete: true,
        noop_update: false,
        create_op: "CreateRole",
        read_structure: Some("Role"),
        read_ops: vec![],
        delete_op: "DeleteRole",
        update_ops: vec![],
        identifier: "RoleName",
        has_tags: true,
        type_overrides: vec![
            ("AssumeRolePolicyDocument", "super::iam_policy_document()"),
            ("Arn", "super::iam_role_arn()"),
            ("RoleId", "super::iam_role_id()"),
        ],
        exclude_fields: vec![
            // Managed policies and inline policies are separate resources
            "PermissionsBoundary",
            "RoleLastUsed",
            "CreateDate",
        ],
        create_only_overrides: vec!["Path", "RoleName"],
        enum_aliases: vec![],
        to_dsl_overrides: vec![],
        required_overrides: vec!["AssumeRolePolicyDocument"],
        extra_read_only: vec!["Arn", "RoleId"],
        read_only_overrides: vec![],
        extra_writable: vec![],
        identity_overrides: vec![],
        derived_attributes: vec![],
    }]
}

/// Returns CloudWatch Logs resource definitions.
pub fn logs_resources() -> Vec<ResourceDef> {
    vec![ResourceDef {
        name: "logs.LogGroup",
        service_namespace: "com.amazonaws.cloudwatchlogs",
        schema_structure: None,
        simple_delete: true,
        noop_update: false,
        create_op: "CreateLogGroup",
        read_structure: Some("LogGroup"),
        read_ops: vec![],
        delete_op: "DeleteLogGroup",
        // PutRetentionPolicy is the real update path for retention_in_days.
        // Listing it here lets the codegen mark the attribute as updatable.
        update_ops: vec![UpdateOp {
            operation: "PutRetentionPolicy",
            fields: FieldLayout::Flat(vec!["retentionInDays"]),
        }],
        identifier: "LogGroupName",
        has_tags: true,
        type_overrides: vec![("KmsKeyId", "super::kms_key_id()"), ("Arn", "super::arn()")],
        exclude_fields: vec![
            "CreationTime",
            "StoredBytes",
            "MetricFilterCount",
            "DataProtectionStatus",
        ],
        create_only_overrides: vec!["LogGroupName", "LogGroupClass"],
        enum_aliases: vec![],
        to_dsl_overrides: vec![],
        required_overrides: vec![],
        extra_read_only: vec!["Arn"],
        read_only_overrides: vec![],
        extra_writable: vec![ExtraField {
            name: "retentionInDays",
            read_source: Some("retentionInDays"),
            description: Some(
                "The number of days to retain the log events in the log group. If unset, events never expire. The provider applies changes via PutRetentionPolicy / DeleteRetentionPolicy.",
            ),
        }],
        identity_overrides: vec![],
        derived_attributes: vec![],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_source_output_field_round_trip() {
        let o = DataSourceOutput {
            name: "arn",
            provider_name: None,
            description: "Bucket ARN",
            type_code: "AttributeType::String",
        };
        assert_eq!(o.name, "arn");
        assert!(o.provider_name.is_none());
        assert_eq!(o.type_code, "AttributeType::String");
    }

    #[test]
    fn sts_caller_identity_declares_outputs_explicitly() {
        let defs = sts_data_sources();
        let ds = &defs[0];
        let names: Vec<&str> = ds.output_attributes.iter().map(|o| o.name).collect();
        assert_eq!(names, vec!["account_id", "arn", "user_id"]);
        let account_id = &ds.output_attributes[0];
        assert_eq!(account_id.provider_name, Some("Account"));
        assert_eq!(account_id.type_code, "super::aws_account_id()");
        let arn = &ds.output_attributes[1];
        assert_eq!(arn.type_code, "super::arn()");
    }

    #[test]
    fn identitystore_user_declares_outputs_explicitly() {
        let defs = identitystore_data_sources();
        let ds = &defs[0];
        let names: Vec<&str> = ds.output_attributes.iter().map(|o| o.name).collect();
        assert_eq!(names, vec!["display_name", "emails"]);
        assert_eq!(ds.output_attributes[0].provider_name, Some("DisplayName"));
        assert_eq!(ds.output_attributes[1].provider_name, Some("Emails"));
        assert_eq!(ds.inputs.len(), 3);
    }

    #[test]
    fn s3_data_sources_declares_s3_bucket() {
        let defs = s3_data_sources();
        assert_eq!(defs.len(), 1);
        let ds = &defs[0];
        assert_eq!(ds.name, "s3.Bucket");
        let inputs: Vec<&str> = ds.inputs.iter().map(|i| i.name).collect();
        assert_eq!(inputs, vec!["bucket"]);
        assert!(ds.inputs[0].required);
        let outputs: Vec<&str> = ds.output_attributes.iter().map(|o| o.name).collect();
        assert_eq!(
            outputs,
            vec![
                "bucket",
                "arn",
                "region",
                "bucket_domain_name",
                "bucket_regional_domain_name",
                "hosted_zone_id"
            ]
        );
        let arn = ds
            .output_attributes
            .iter()
            .find(|o| o.name == "arn")
            .unwrap();
        assert!(arn.provider_name.is_none());
        let region = ds
            .output_attributes
            .iter()
            .find(|o| o.name == "region")
            .unwrap();
        assert_eq!(region.provider_name, Some("LocationConstraint"));
    }

    #[test]
    fn data_source_def_carries_output_attributes() {
        let def = DataSourceDef {
            name: "test.X",
            service_namespace: "com.test",
            inputs: vec![],
            output_attributes: vec![DataSourceOutput {
                name: "arn",
                provider_name: None,
                description: "",
                type_code: "AttributeType::String",
            }],
            read_ops: vec![],
            type_overrides: vec![],
            exclude_fields: vec![],
        };
        assert_eq!(def.output_attributes.len(), 1);
        assert_eq!(def.output_attributes[0].name, "arn");
    }
}
