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
    /// Extra schema attributes (writable or read-only) that are not present
    /// in the create operation input. Each entry is emitted as an
    /// `AttributeSchema::new(...)` line with the usual flag combination:
    /// - `update_ops` lists the field → updatable (else create-only).
    /// - `read_only_overrides` lists the field → read-only.
    pub extra_attributes: Vec<ExtraField>,
    /// Fields to mark as identity (contribute to anonymous resource identifier hashing).
    /// Use for attributes that distinguish same-type resources sharing create-only values.
    pub identity_overrides: Vec<&'static str>,
    /// Fields whose value the AWS API populates *asynchronously after Create
    /// returns* — chained references to these from another resource will be
    /// rejected at validate time unless the user has declared a `wait`
    /// block on the binding (carina#3034). Examples: ACM `Status`
    /// (PENDING_VALIDATION → ISSUED), CloudFront `DomainName`,
    /// RDS `Endpoint`. Names are PascalCase, matching the rest of
    /// `ResourceDef`'s `*_overrides` Vecs.
    pub deferred_populate_overrides: Vec<&'static str>,
    /// Per-(field, member) overrides for `StructField.deferred_populate`
    /// inside a top-level attribute. Used when only specific inner
    /// fields of a struct/list-of-struct attribute populate
    /// asynchronously, not the whole attribute (carina#3034).
    ///
    /// Each tuple is `(field_pascal_name, member_pascal_name)`. The
    /// codegen looks up the rendered struct's StructField list and
    /// adds `.deferred_populate()` to the matching member.
    ///
    /// Example: ACM `Certificate.DomainValidationOptions[*].ResourceRecord`
    /// is populated asynchronously after RequestCertificate; the user
    /// dereferences `cert.domain_validation_options[0].resource_record`
    /// in a `route53.RecordSet` and needs validate to flag it without
    /// a `wait`.
    pub deferred_populate_struct_field_overrides: Vec<(&'static str, &'static str)>,
    /// Per-(parent_struct, member) type overrides for `StructField.field_type`.
    /// Distinguishes a struct field by the *parent* struct shape, so a
    /// member name that appears in multiple structs gets the right type
    /// in each context.
    ///
    /// Each tuple is `(parent_struct_pascal_name, member_pascal_name,
    /// type_code)`. `type_code` is a Rust expression that evaluates to
    /// an `AttributeType` — typically a call into `super::types::…`.
    ///
    /// Example: route53 `AliasTarget.HostedZoneId` accepts the
    /// `aws.cloudfront.HostedZoneId.global` namespaced constant
    /// (resolving to `Z2FDTNDATAQYW2`), while the top-level
    /// `RecordSet.HostedZoneId` is the user-supplied Route 53 zone ID
    /// and stays plain String (aws#302).
    pub struct_field_type_overrides: Vec<(&'static str, &'static str, &'static str)>,
    /// Fields whose schema type should come from the *read structure*
    /// rather than the create input — for attributes where the AWS
    /// API's request shape differs from its response shape and only
    /// the response shape is what downstream DSL chained accesses
    /// reference (carina-provider-aws#296).
    ///
    /// Example: ACM `Certificate.DomainValidationOptions` is typed
    /// `List<DomainValidationOption{domain_name, validation_domain}>`
    /// in the create input but
    /// `List<DomainValidation{domain_name, resource_record{name, type, value}, validation_status, ...}>`
    /// in the read response (`CertificateDetail`). The read shape is
    /// the one users want to chain into (the carina#3032 trigger
    /// case). Without this override the codegen picks the create-side
    /// shape, the read-side `resource_record_*` fields are missing
    /// from the generated schema, and the type-narrowing pass cannot
    /// see them.
    ///
    /// Implies the attribute is treated as read-only (the rich
    /// read-side shape cannot be round-tripped back into the create
    /// input). Names are PascalCase.
    pub read_shape_overrides: Vec<&'static str>,
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
    /// `obj.<list_member>().iter().filter_map(|x| x.<child_member>())`.
    /// Used to flatten a list-of-struct response field into a
    /// `Value::List(String)` attribute by projecting one member off each
    /// element (e.g. `VpcEndpoint.Groups[*].GroupId` → `security_group_ids`).
    /// The attribute is omitted entirely when the projection yields an
    /// empty list.
    ListAll {
        list_member: &'static str,
        child_member: &'static str,
    },
    /// `obj.<struct_member>().<child_member>()` flattened into the
    /// top-level attribute map under the DSL name in
    /// `DerivedAttribute::attr` — so renames work (e.g.
    /// `VpcPeeringConnection.AccepterVpcInfo.VpcId` → `peer_vpc_id`).
    /// One declaration per child; the same `struct_member` may appear
    /// in multiple `DerivedAttribute` entries to flatten multiple
    /// children of the same nested struct.
    Struct {
        struct_member: &'static str,
        child_member: &'static str,
    },
    /// `obj.<struct_member>()` collected into a single `Value::Map`
    /// attribute keyed by `DerivedAttribute::attr`. Each listed child
    /// member contributes one entry; per-child kinds (String / Bool /
    /// Int / Long / Enum) are inferred from the inner struct shape.
    /// The outer attribute is omitted when no children are populated.
    StructAsMap {
        struct_member: &'static str,
        children: &'static [&'static str],
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
    /// Output shape: a single item (scalar attributes) or a list-aggregated
    /// result (each attribute is `list(T)` aggregated across paginated API calls).
    pub shape: DataSourceShape,
}

/// Output shape of a data source.
///
/// Splits "single-item lookup" from "paginated list lookup with client-side
/// filter" so each variant carries only the fields that make sense for it.
/// The hand-written read implementation under `services/<svc>/<resource>.rs`
/// is responsible for honoring the shape contract — codegen only emits the
/// matching schema/docs.
pub enum DataSourceShape {
    /// One AWS API response → scalar attributes (e.g. `sts.CallerIdentity`,
    /// `identitystore.User`, `s3.Bucket`).
    Single {
        /// Declared output attributes (read-only fields exposed to the DSL).
        output_attributes: Vec<DataSourceOutput>,
    },
    /// Paginated list API + client-side filter → each attribute is `list(T)`,
    /// aggregated across pages (e.g. `iam.Roles` over `ListRoles`).
    ListAggregated {
        /// Declared aggregated output attributes. Each becomes
        /// `AttributeType::List { inner: item_type, ordered: false }`.
        aggregated_outputs: Vec<DataSourceListOutput>,
    },
}

impl DataSourceShape {
    /// Returns the `Single` shape's outputs, or `None` for `ListAggregated`.
    /// Callers must explicitly handle the `None` arm — there is no silent
    /// "empty slice for the wrong shape" fallback.
    pub fn single_outputs(&self) -> Option<&[DataSourceOutput]> {
        match self {
            DataSourceShape::Single { output_attributes } => Some(output_attributes),
            DataSourceShape::ListAggregated { .. } => None,
        }
    }
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
    /// Type override (e.g., "AttributeType::string()"). None = infer from Smithy.
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
    /// Rust type expression for codegen, e.g. `"AttributeType::string()"` or
    /// `"super::aws_account_id()"`. Required: codegen does not infer.
    pub type_code: &'static str,
}

/// One aggregated output attribute on a `DataSourceShape::ListAggregated`.
///
/// The runtime implementation must populate this attribute with
/// `Value::Concrete(ConcreteValue::List(...))` whose items are of `item_type`.
/// Codegen wraps `item_type` in `AttributeType::List { inner, ordered: false }`
/// when emitting the schema.
pub struct DataSourceListOutput {
    /// DSL field name (e.g., "arns")
    pub name: &'static str,
    /// Human-readable description for docs
    pub description: &'static str,
    /// Rust type expression for the *element* type, e.g.
    /// `"AttributeType::string()"` or `"super::arn()"`. Codegen wraps it in
    /// `AttributeType::List { inner: <item_type>, ordered: false }`.
    pub item_type: &'static str,
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            // private_dns_name_options_on_launch is a nested struct on the
            // read shape; collapse it into a single Value::Map attribute
            // so the DSL surface stays flat.
            derived_attributes: vec![DerivedAttribute {
                attr: "PrivateDnsNameOptionsOnLaunch",
                source: DerivedSource::StructAsMap {
                    struct_member: "PrivateDnsNameOptionsOnLaunch",
                    children: &[
                        "HostnameType",
                        "EnableResourceNameDnsARecord",
                        "EnableResourceNameDnsAAAARecord",
                    ],
                },
            }],
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
            extra_attributes: vec![ExtraField {
                name: "VpcId",
                read_source: None,
                description: Some(
                    "The ID of the VPC to attach the internet gateway to. The provider attaches the IGW after creation and detaches before deletion.",
                ),
            }],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            // dsl_aliases auto-derives hyphen->underscore via dsl_enum_value;
            // explicit (-1, all) flows in via enum_aliases.
            to_dsl_overrides: vec![],
            required_overrides: vec!["IpProtocol"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
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
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            // dsl_aliases auto-derives hyphen->underscore; explicit (-1, all)
            // flows in via enum_aliases.
            to_dsl_overrides: vec![],
            required_overrides: vec!["IpProtocol", "GroupId"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
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
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            // dsl_aliases auto-derives hyphen->underscore from values; no override needed.
            to_dsl_overrides: vec![],
            required_overrides: vec!["ResourceId", "ResourceType"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            // The TransitGatewayRequestOptions sub-struct on the response
            // is flattened into top-level attributes (matching the DSL
            // surface, which doesn't expose `options` as a nested struct).
            derived_attributes: vec![
                DerivedAttribute {
                    attr: "AmazonSideAsn",
                    source: DerivedSource::Struct {
                        struct_member: "Options",
                        child_member: "AmazonSideAsn",
                    },
                },
                DerivedAttribute {
                    attr: "AutoAcceptSharedAttachments",
                    source: DerivedSource::Struct {
                        struct_member: "Options",
                        child_member: "AutoAcceptSharedAttachments",
                    },
                },
                DerivedAttribute {
                    attr: "DefaultRouteTableAssociation",
                    source: DerivedSource::Struct {
                        struct_member: "Options",
                        child_member: "DefaultRouteTableAssociation",
                    },
                },
                DerivedAttribute {
                    attr: "DefaultRouteTablePropagation",
                    source: DerivedSource::Struct {
                        struct_member: "Options",
                        child_member: "DefaultRouteTablePropagation",
                    },
                },
                DerivedAttribute {
                    attr: "DnsSupport",
                    source: DerivedSource::Struct {
                        struct_member: "Options",
                        child_member: "DnsSupport",
                    },
                },
                DerivedAttribute {
                    attr: "VpnEcmpSupport",
                    source: DerivedSource::Struct {
                        struct_member: "Options",
                        child_member: "VpnEcmpSupport",
                    },
                },
            ],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            // SecurityGroupIds is a write-side input
            // (CreateVpcEndpointRequest.SecurityGroupIds: List<String>) but
            // the read structure exposes it as Groups[*].GroupId on a
            // SecurityGroupIdentifier struct, not as a direct member.
            derived_attributes: vec![DerivedAttribute {
                attr: "SecurityGroupIds",
                source: DerivedSource::ListAll {
                    list_member: "Groups",
                    child_member: "GroupId",
                },
            }],
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
            extra_attributes: vec![
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
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            // The peering response carries the requester / accepter VPCs
            // in two parallel `VpcPeeringConnectionVpcInfo` sub-structs;
            // the DSL surface flattens both, with the accepter-side
            // children renamed to `peer_*` so the user can tell them apart.
            derived_attributes: vec![
                DerivedAttribute {
                    attr: "VpcId",
                    source: DerivedSource::Struct {
                        struct_member: "RequesterVpcInfo",
                        child_member: "VpcId",
                    },
                },
                DerivedAttribute {
                    attr: "PeerVpcId",
                    source: DerivedSource::Struct {
                        struct_member: "AccepterVpcInfo",
                        child_member: "VpcId",
                    },
                },
                DerivedAttribute {
                    attr: "PeerOwnerId",
                    source: DerivedSource::Struct {
                        struct_member: "AccepterVpcInfo",
                        child_member: "OwnerId",
                    },
                },
                DerivedAttribute {
                    attr: "PeerRegion",
                    source: DerivedSource::Struct {
                        struct_member: "AccepterVpcInfo",
                        child_member: "Region",
                    },
                },
            ],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
        shape: DataSourceShape::Single {
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
                    type_code: "AttributeType::string()",
                },
            ],
        },
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
        shape: DataSourceShape::Single {
            output_attributes: vec![
                DataSourceOutput {
                    name: "display_name",
                    provider_name: Some("DisplayName"),
                    description: "Display name of the user.",
                    type_code: "AttributeType::string()",
                },
                DataSourceOutput {
                    name: "emails",
                    provider_name: Some("Emails"),
                    description: "Email addresses associated with the user.",
                    type_code: "AttributeType::string()",
                },
            ],
        },
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
            // Versioning, ACL, ownership controls and object lock were
            // previously bundled here; they now live in standalone resources
            // (s3.BucketVersioning #211, s3.BucketAcl + s3.BucketOwnershipControls #213).
            read_ops: vec![],
            delete_op: "DeleteBucket",
            update_ops: vec![],
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
                // Bundled attrs moved to standalone resources (#213):
                "ACL",
                "GrantFullControl",
                "GrantRead",
                "GrantReadACP",
                "GrantWrite",
                "GrantWriteACP",
                "ObjectOwnership",
                "ObjectLockEnabledForBucket",
            ],
            create_only_overrides: vec![],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec![],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketPolicy — attaches a resource-based policy to an existing
        // S3 bucket. Maps to PutBucketPolicy / GetBucketPolicy /
        // DeleteBucketPolicy.
        ResourceDef {
            name: "s3.BucketPolicy",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            // Delete is hand-written in services/s3/bucket_policy.rs as
            // delete_s3_bucket_policy_idempotent, which treats
            // NoSuchBucketPolicy / NoSuchBucket as success so destroy retries
            // succeed when the policy or its parent bucket is already gone.
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketPolicy",
            read_structure: None,
            // Read is hand-written in services/s3/bucket_policy.rs to convert
            // the JSON policy to a typed Value::Map; the generated per-op read
            // would emit it as Value::String and bypass that conversion.
            read_ops: vec![],
            delete_op: "DeleteBucketPolicy",
            update_ops: vec![UpdateOp {
                operation: "PutBucketPolicy",
                fields: FieldLayout::Flat(vec!["Policy"]),
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![("Policy", "super::iam_policy_document()")],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ConfirmRemoveSelfBucketAccess",
                "ExpectedBucketOwner",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "Policy"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketVersioning — controls the versioning configuration of an
        // existing S3 bucket. Maps to PutBucketVersioning / GetBucketVersioning.
        // There is no DeleteBucketVersioning API; destroy is implemented as
        // PutBucketVersioning with Status=Suspended.
        ResourceDef {
            name: "s3.BucketVersioning",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketVersioning",
            read_structure: None,
            read_ops: vec![],
            // Suspend-on-delete: there is no DeleteBucketVersioning API.
            // The hand-written delete method calls PutBucketVersioning with
            // Status=Suspended; this delete_op string is a no-op for codegen
            // because simple_delete is false.
            delete_op: "PutBucketVersioning",
            update_ops: vec![UpdateOp {
                operation: "PutBucketVersioning",
                fields: FieldLayout::InsideStruct {
                    name: "VersioningConfiguration",
                    fields: vec!["Status", "MFADelete"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![
                (
                    "Status",
                    "AttributeType::string_enum(\"VersioningStatus\".to_string(), vec![\"Enabled\".to_string(), \"Suspended\".to_string()], Some(carina_core::schema::string_enum_identity(\"VersioningStatus\", Some(\"aws.s3.BucketVersioning\"))), vec![(\"Enabled\".to_string(), \"enabled\".to_string()), (\"Suspended\".to_string(), \"suspended\".to_string())])",
                ),
                (
                    "MFADelete",
                    "AttributeType::string_enum(\"MFADelete\".to_string(), vec![\"Enabled\".to_string(), \"Disabled\".to_string()], Some(carina_core::schema::string_enum_identity(\"MFADelete\", Some(\"aws.s3.BucketVersioning\"))), vec![(\"Enabled\".to_string(), \"enabled\".to_string()), (\"Disabled\".to_string(), \"disabled\".to_string())])",
                ),
            ],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "MFA",
                "ExpectedBucketOwner",
                "VersioningConfiguration",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "Status"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
                ExtraField {
                    name: "Status",
                    read_source: None,
                    description: Some("Versioning state of the bucket: Enabled or Suspended."),
                },
                ExtraField {
                    name: "MFADelete",
                    read_source: None,
                    description: Some(
                        "MFA-delete state. Specifies whether MFA delete is enabled in the bucket versioning configuration.",
                    ),
                },
            ],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketAcl — sets the canned ACL on an existing S3 bucket.
        // Maps to PutBucketAcl / GetBucketAcl. There is no DeleteBucketAcl
        // API; "destroying" the resource resets the ACL to "private".
        ResourceDef {
            name: "s3.BucketAcl",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketAcl",
            read_structure: None,
            read_ops: vec![],
            delete_op: "PutBucketAcl",
            update_ops: vec![UpdateOp {
                operation: "PutBucketAcl",
                fields: FieldLayout::Flat(vec!["ACL"]),
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                // The full grant-by-header form and AccessControlPolicy are
                // out of scope for the initial split; canned ACL only.
                "GrantFullControl",
                "GrantRead",
                "GrantReadACP",
                "GrantWrite",
                "GrantWriteACP",
                "AccessControlPolicy",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![
                ("acl", "authenticated_read", "authenticated-read"),
                ("acl", "public_read", "public-read"),
                ("acl", "public_read_write", "public-read-write"),
            ],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "ACL"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketOwnershipControls — sets the ObjectOwnership setting on
        // an existing S3 bucket. Maps to PutBucketOwnershipControls /
        // GetBucketOwnershipControls / DeleteBucketOwnershipControls.
        ResourceDef {
            name: "s3.BucketOwnershipControls",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketOwnershipControls",
            read_structure: None,
            read_ops: vec![],
            delete_op: "DeleteBucketOwnershipControls",
            update_ops: vec![UpdateOp {
                operation: "PutBucketOwnershipControls",
                fields: FieldLayout::InsideStruct {
                    name: "OwnershipControls",
                    fields: vec!["ObjectOwnership"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![(
                "ObjectOwnership",
                "AttributeType::string_enum(\"ObjectOwnership\".to_string(), vec![\"BucketOwnerEnforced\".to_string(), \"BucketOwnerPreferred\".to_string(), \"ObjectWriter\".to_string()], Some(carina_core::schema::string_enum_identity(\"ObjectOwnership\", Some(\"aws.s3.BucketOwnershipControls\"))), vec![(\"BucketOwnerEnforced\".to_string(), \"bucket_owner_enforced\".to_string()), (\"BucketOwnerPreferred\".to_string(), \"bucket_owner_preferred\".to_string()), (\"ObjectWriter\".to_string(), \"object_writer\".to_string())])",
            )],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                "OwnershipControls",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "ObjectOwnership"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![ExtraField {
                name: "ObjectOwnership",
                read_source: None,
                description: Some(
                    "Object ownership setting: BucketOwnerEnforced, BucketOwnerPreferred, or ObjectWriter.",
                ),
            }],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketServerSideEncryptionConfiguration — controls the SSE
        // configuration of an existing S3 bucket. Maps to PutBucketEncryption /
        // GetBucketEncryption / DeleteBucketEncryption.
        ResourceDef {
            name: "s3.BucketServerSideEncryptionConfiguration",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketEncryption",
            read_structure: None,
            // Read / Create / Update / Delete are hand-written; the rules
            // attribute is a typed Struct list (carina_aws_types::bucket_encryption_rules)
            // that the generic codegen body cannot construct.
            read_ops: vec![],
            delete_op: "DeleteBucketEncryption",
            // Declare PutBucketEncryption as the update op so codegen marks
            // `Rules` as updatable; the generated write helper is suppressed
            // because `Rules` has a `type_overrides` entry.
            update_ops: vec![UpdateOp {
                operation: "PutBucketEncryption",
                fields: FieldLayout::InsideStruct {
                    name: "ServerSideEncryptionConfiguration",
                    fields: vec!["Rules"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![("Rules", "super::bucket_encryption_rules()")],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                "ServerSideEncryptionConfiguration",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "Rules"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![ExtraField {
                name: "Rules",
                read_source: None,
                description: Some("List of server-side encryption rules to apply to the bucket."),
            }],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketReplicationConfiguration — controls cross-region (or
        // same-region) replication. Maps to PutBucketReplication /
        // GetBucketReplication / DeleteBucketReplication.
        ResourceDef {
            name: "s3.BucketReplicationConfiguration",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketReplication",
            read_structure: None,
            read_ops: vec![],
            delete_op: "DeleteBucketReplication",
            update_ops: vec![UpdateOp {
                operation: "PutBucketReplication",
                fields: FieldLayout::InsideStruct {
                    name: "ReplicationConfiguration",
                    fields: vec!["Role", "Rules"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![("Rules", "super::bucket_replication_rules()")],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "Token",
                "ExpectedBucketOwner",
                "ReplicationConfiguration",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "Role", "Rules"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
                ExtraField {
                    name: "Role",
                    read_source: None,
                    description: Some("ARN of the IAM role S3 uses to perform the replication."),
                },
                ExtraField {
                    name: "Rules",
                    read_source: None,
                    description: Some("Replication rules — at least one is required."),
                },
            ],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketLifecycleConfiguration — controls object lifecycle rules
        // (transitions, expiration, abort-multipart). Maps to
        // PutBucketLifecycleConfiguration / GetBucketLifecycleConfiguration /
        // DeleteBucketLifecycle.
        ResourceDef {
            name: "s3.BucketLifecycleConfiguration",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketLifecycleConfiguration",
            read_structure: None,
            read_ops: vec![],
            delete_op: "DeleteBucketLifecycle",
            update_ops: vec![UpdateOp {
                operation: "PutBucketLifecycleConfiguration",
                fields: FieldLayout::InsideStruct {
                    name: "LifecycleConfiguration",
                    fields: vec!["Rules"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![("Rules", "super::bucket_lifecycle_rules()")],
            exclude_fields: vec![
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                "LifecycleConfiguration",
                "TransitionDefaultMinimumObjectSize",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "Rules"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![ExtraField {
                name: "Rules",
                read_source: None,
                description: Some("Lifecycle rules to apply to the bucket."),
            }],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketWebsiteConfiguration — configures static-website hosting.
        // Maps to PutBucketWebsite / GetBucketWebsite / DeleteBucketWebsite.
        ResourceDef {
            name: "s3.BucketWebsiteConfiguration",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketWebsite",
            read_structure: None,
            read_ops: vec![],
            delete_op: "DeleteBucketWebsite",
            update_ops: vec![UpdateOp {
                operation: "PutBucketWebsite",
                fields: FieldLayout::InsideStruct {
                    name: "WebsiteConfiguration",
                    fields: vec!["IndexDocument", "ErrorDocument", "RedirectAllRequestsTo"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![
                ("IndexDocument", "super::s3_index_document()"),
                ("ErrorDocument", "super::s3_error_document()"),
                (
                    "RedirectAllRequestsTo",
                    "super::s3_redirect_all_requests_to()",
                ),
            ],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                "WebsiteConfiguration",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
                ExtraField {
                    name: "IndexDocument",
                    read_source: None,
                    description: Some("Index document for the bucket's website."),
                },
                ExtraField {
                    name: "ErrorDocument",
                    read_source: None,
                    description: Some("Custom error document key."),
                },
                ExtraField {
                    name: "RedirectAllRequestsTo",
                    read_source: None,
                    description: Some(
                        "Redirect all bucket-website requests to another host (alternative to index_document).",
                    ),
                },
            ],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketCorsConfiguration — controls CORS rules on an existing
        // S3 bucket. Maps to PutBucketCors / GetBucketCors / DeleteBucketCors.
        ResourceDef {
            name: "s3.BucketCorsConfiguration",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketCors",
            read_structure: None,
            read_ops: vec![],
            delete_op: "DeleteBucketCors",
            update_ops: vec![UpdateOp {
                operation: "PutBucketCors",
                fields: FieldLayout::InsideStruct {
                    name: "CORSConfiguration",
                    fields: vec!["CORSRules"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![("CORSRules", "super::bucket_cors_rules()")],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                "CORSConfiguration",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "CORSRules"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![ExtraField {
                name: "CORSRules",
                read_source: None,
                description: Some("CORS rules to apply to the bucket."),
            }],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketNotificationConfiguration — wires bucket events to
        // SNS / SQS / Lambda / EventBridge destinations. Maps to
        // PutBucketNotificationConfiguration / GetBucketNotificationConfiguration.
        // No DeleteBucket* API exists; destroy is implemented as a Put with
        // empty configuration (Terraform parity).
        ResourceDef {
            name: "s3.BucketNotificationConfiguration",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketNotificationConfiguration",
            read_structure: None,
            read_ops: vec![],
            // No native delete API — destroy uses the same Put op with
            // empty lists; we still set delete_op for code-gen totality but
            // the dispatch table calls the hand-written idempotent helper.
            delete_op: "PutBucketNotificationConfiguration",
            update_ops: vec![UpdateOp {
                operation: "PutBucketNotificationConfiguration",
                fields: FieldLayout::InsideStruct {
                    name: "NotificationConfiguration",
                    fields: vec![
                        "TopicConfigurations",
                        "QueueConfigurations",
                        "LambdaFunctionConfigurations",
                        "EventBridgeConfiguration",
                    ],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![
                (
                    "TopicConfigurations",
                    "super::bucket_topic_configurations()",
                ),
                (
                    "QueueConfigurations",
                    "super::bucket_queue_configurations()",
                ),
                (
                    "LambdaFunctionConfigurations",
                    "super::bucket_lambda_function_configurations()",
                ),
                (
                    "EventBridgeConfiguration",
                    "super::bucket_event_bridge_configuration()",
                ),
            ],
            exclude_fields: vec![
                "ExpectedBucketOwner",
                "SkipDestinationValidation",
                "NotificationConfiguration",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
                ExtraField {
                    name: "TopicConfigurations",
                    read_source: None,
                    description: Some("SNS topic notification configurations."),
                },
                ExtraField {
                    name: "QueueConfigurations",
                    read_source: None,
                    description: Some("SQS queue notification configurations."),
                },
                ExtraField {
                    name: "LambdaFunctionConfigurations",
                    read_source: None,
                    description: Some("Lambda function notification configurations."),
                },
                ExtraField {
                    name: "EventBridgeConfiguration",
                    read_source: None,
                    description: Some("Enables EventBridge notifications when present."),
                },
            ],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketLogging — controls server-access logging on a bucket.
        // Maps to PutBucketLogging / GetBucketLogging. No DeleteBucketLogging
        // API exists; destroy is implemented as a Put with an empty
        // BucketLoggingStatus (Terraform parity).
        ResourceDef {
            name: "s3.BucketLogging",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            simple_delete: false,
            noop_update: false,
            create_op: "PutBucketLogging",
            read_structure: None,
            read_ops: vec![],
            // No native delete API — destroy uses the same Put op.
            delete_op: "PutBucketLogging",
            update_ops: vec![UpdateOp {
                operation: "PutBucketLogging",
                fields: FieldLayout::InsideStruct {
                    name: "LoggingEnabled",
                    fields: vec!["TargetBucket", "TargetPrefix", "TargetObjectKeyFormat"],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![(
                "TargetObjectKeyFormat",
                "super::bucket_target_object_key_format()",
            )],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                "BucketLoggingStatus",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket", "TargetBucket"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
                ExtraField {
                    name: "TargetBucket",
                    read_source: None,
                    description: Some("Destination bucket for server access logs."),
                },
                ExtraField {
                    name: "TargetPrefix",
                    read_source: None,
                    description: Some("Key prefix to apply to log objects."),
                },
                ExtraField {
                    name: "TargetObjectKeyFormat",
                    read_source: None,
                    description: Some("Partitioning / simple-prefix selector for log object keys."),
                },
            ],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
        // s3.BucketPublicAccessBlock — controls the Public Access Block
        // settings of an existing S3 bucket. Maps to PutPublicAccessBlock /
        // GetPublicAccessBlock / DeletePublicAccessBlock.
        ResourceDef {
            name: "s3.BucketPublicAccessBlock",
            service_namespace: "com.amazonaws.s3",
            schema_structure: None,
            // simple_delete: false because delete is hand-written
            // (delete_s3_bucket_public_access_block_idempotent) — it treats
            // NoSuchPublicAccessBlockConfiguration / NoSuchBucket as success
            // so destroy retries succeed when the configuration or its
            // parent bucket is already gone.
            simple_delete: false,
            noop_update: false,
            create_op: "PutPublicAccessBlock",
            read_structure: None,
            // Read / Create / Update / Delete are hand-written in
            // services/s3/bucket_public_access_block.rs because the AWS
            // SDK's PublicAccessBlockConfiguration takes typed `bool`
            // setters that the generic codegen-emitted write function
            // (which assumes Value::String) cannot satisfy.
            read_ops: vec![],
            delete_op: "DeletePublicAccessBlock",
            update_ops: vec![UpdateOp {
                operation: "PutPublicAccessBlock",
                fields: FieldLayout::InsideStruct {
                    name: "PublicAccessBlockConfiguration",
                    fields: vec![
                        "BlockPublicAcls",
                        "IgnorePublicAcls",
                        "BlockPublicPolicy",
                        "RestrictPublicBuckets",
                    ],
                },
            }],
            identifier: "Bucket",
            has_tags: false,
            type_overrides: vec![
                ("BlockPublicAcls", "AttributeType::bool()"),
                ("IgnorePublicAcls", "AttributeType::bool()"),
                ("BlockPublicPolicy", "AttributeType::bool()"),
                ("RestrictPublicBuckets", "AttributeType::bool()"),
            ],
            exclude_fields: vec![
                "ContentMD5",
                "ChecksumAlgorithm",
                "ExpectedBucketOwner",
                "PublicAccessBlockConfiguration",
            ],
            create_only_overrides: vec!["Bucket"],
            enum_aliases: vec![],
            to_dsl_overrides: vec![],
            required_overrides: vec!["Bucket"],
            extra_read_only: vec![],
            read_only_overrides: vec![],
            extra_attributes: vec![
                ExtraField {
                    name: "BlockPublicAcls",
                    read_source: None,
                    description: Some("Block public ACLs on the bucket and its objects."),
                },
                ExtraField {
                    name: "IgnorePublicAcls",
                    read_source: None,
                    description: Some("Ignore any public ACLs on the bucket and its objects."),
                },
                ExtraField {
                    name: "BlockPublicPolicy",
                    read_source: None,
                    description: Some("Block public bucket policies for the bucket."),
                },
                ExtraField {
                    name: "RestrictPublicBuckets",
                    read_source: None,
                    description: Some(
                        "Restrict access to the bucket to AWS service principals and authorized users when a public policy is set.",
                    ),
                },
            ],
            identity_overrides: vec![],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            struct_field_type_overrides: vec![],
            read_shape_overrides: vec![],
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
        shape: DataSourceShape::Single {
            output_attributes: vec![
                DataSourceOutput {
                    name: "bucket",
                    provider_name: None,
                    description: "The bucket name (echo of the input).",
                    type_code: "AttributeType::string()",
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
                    type_code: "AttributeType::string()",
                },
                DataSourceOutput {
                    name: "bucket_domain_name",
                    provider_name: None,
                    description: "Bucket domain name (`<bucket>.s3.amazonaws.com`).",
                    type_code: "AttributeType::string()",
                },
                DataSourceOutput {
                    name: "bucket_regional_domain_name",
                    provider_name: None,
                    description: "Region-specific bucket domain name (`<bucket>.s3.<region>.amazonaws.com`).",
                    type_code: "AttributeType::string()",
                },
                DataSourceOutput {
                    name: "hosted_zone_id",
                    provider_name: None,
                    description: "Route 53 Hosted Zone ID for the bucket's region.",
                    type_code: "AttributeType::string()",
                },
            ],
        },
    }]
}

/// Returns ACM resource definitions.
///
/// `aws.acm.Certificate` covers the request-issue-renew lifecycle of an
/// ACM certificate. CertificateValidation is intentionally NOT a
/// separate waiter resource — the wait-construct (carina#2825) is the
/// canonical way to block downstream resources on `status == ISSUED`.
/// See carina-rs/carina-provider-aws#244 for the design discussion.
pub fn acm_resources() -> Vec<ResourceDef> {
    vec![ResourceDef {
        name: "acm.Certificate",
        service_namespace: "com.amazonaws.acm",
        schema_structure: None,
        simple_delete: true,
        // ACM `UpdateCertificateOptions` is narrow (CT logging + export
        // preference). Other field updates require replacement.
        noop_update: false,
        create_op: "RequestCertificate",
        read_structure: Some("CertificateDetail"),
        read_ops: vec![],
        delete_op: "DeleteCertificate",
        update_ops: vec![],
        identifier: "CertificateArn",
        has_tags: true,
        type_overrides: vec![],
        exclude_fields: vec![
            // Imported / private-CA flows are out of scope for the
            // initial cut. The wait construct is the validation path,
            // not these legacy fields.
            "CertificateAuthorityArn",
            "PrivateKey",
            "Certificate",
            "CertificateChain",
            "Passphrase",
            "ExtendedKeyUsages",
            "InUseBy",
            "IssuedAt",
            "ImportedAt",
            "RevokedAt",
            "RevocationReason",
            "CreatedAt",
            "NotAfter",
            "NotBefore",
            "Issuer",
            "Subject",
            "Signature",
            "SignatureAlgorithm",
            "Serial",
            "ManagedBy",
            "ExportOption",
            "ExportablePrivateKey",
            "Managed",
            "KeyUsages",
            "FailureReason",
        ],
        // RequestCertificate-only fields. Most ACM fields cannot be
        // mutated post-issue without re-requesting the certificate.
        create_only_overrides: vec![
            "DomainName",
            "SubjectAlternativeNames",
            "ValidationMethod",
            "KeyAlgorithm",
            "DomainValidationOptions",
            "IdempotencyToken",
        ],
        enum_aliases: vec![],
        to_dsl_overrides: vec![],
        required_overrides: vec!["DomainName"],
        extra_read_only: vec![
            "Status",
            "DomainValidationOptions",
            "Type",
            "RenewalEligibility",
            "RenewalSummary",
        ],
        read_only_overrides: vec![],
        extra_attributes: vec![],
        identity_overrides: vec![],
        // ACM transitions Status PENDING_VALIDATION → ISSUED
        // asynchronously after RequestCertificate returns. Downstream
        // resources that read `cert.status` directly without a
        // synchronizing `wait` block hit the carina#3032 failure
        // mode at apply time. carina#3034 marks this here so
        // validate flags it before apply.
        deferred_populate_overrides: vec!["Status"],
        // The ACM read-back populates `DomainValidationOptions[*]
        // .ResourceRecord` only after the validation step has computed
        // the CNAME — so a downstream RecordSet that reads
        // `cert.domain_validation_options[0].resource_record` directly
        // hits the carina#3032 failure mode just like `Status`. Mark
        // the inner StructField asynchronous so validate flags it.
        // Requires `read_shape_overrides` below to surface the
        // `ResourceRecord` field from the read structure in the first
        // place — the request-shape `DomainValidationOption` does not
        // have it.
        deferred_populate_struct_field_overrides: vec![(
            "DomainValidationOptions",
            "ResourceRecord",
        )],
        struct_field_type_overrides: vec![],
        // ACM's `DomainValidationOptions` is one of the rare AWS
        // attributes whose request shape is meaningfully narrower
        // than its response shape. The create input takes a list of
        // `(domain_name, validation_domain)` tuples (used only when
        // overriding the email validation domain); the read response
        // returns a list of richer entries with `resource_record`
        // (the CNAME the user must publish), `validation_status`,
        // etc. — the part downstream resources actually want to
        // chain into. carina-rs/carina#3032 trigger lives here.
        read_shape_overrides: vec!["DomainValidationOptions"],
        derived_attributes: vec![],
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
                    "AttributeType::list(AttributeType::string())",
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
            extra_attributes: vec![ExtraField {
                name: "HostedZoneId",
                read_source: None,
                description: Some("The ID of the hosted zone that contains this record set."),
            }],
            identity_overrides: vec!["Type"],
            deferred_populate_overrides: vec![],
            deferred_populate_struct_field_overrides: vec![],
            // `AliasTarget.HostedZoneId` accepts the `aws.cloudfront.HostedZoneId.global`
            // namespaced constant (resolved to `Z2FDTNDATAQYW2`) in addition to literal
            // Route 53 hosted-zone IDs. The top-level `RecordSet.HostedZoneId` (the
            // user's own Route 53 zone) is intentionally left as plain String (aws#302).
            struct_field_type_overrides: vec![(
                "AliasTarget",
                "HostedZoneId",
                "super::cloudfront_hosted_zone_id()",
            )],
            read_shape_overrides: vec![],
            derived_attributes: vec![],
        },
    ]
}

/// Returns IAM data source definitions.
pub fn iam_data_sources() -> Vec<DataSourceDef> {
    vec![DataSourceDef {
        name: "iam.Roles",
        service_namespace: "com.amazonaws.iam",
        inputs: vec![
            DataSourceInput {
                name: "path_prefix",
                provider_name: "PathPrefix",
                description: "Path prefix for filtering the results. If it is not included, it defaults to `/`, listing all roles.",
                required: false,
                type_override: None,
            },
            DataSourceInput {
                name: "name_regex",
                provider_name: "",
                description: "Regular expression string applied client-side to filter results by role name.",
                required: false,
                type_override: None,
            },
        ],
        shape: DataSourceShape::ListAggregated {
            aggregated_outputs: vec![
                DataSourceListOutput {
                    name: "arns",
                    description: "Set of ARNs of the matched IAM roles.",
                    item_type: "super::iam_role_arn()",
                },
                DataSourceListOutput {
                    name: "names",
                    description: "Set of names of the matched IAM roles.",
                    item_type: "AttributeType::string()",
                },
            ],
        },
    }]
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
        extra_attributes: vec![],
        identity_overrides: vec![],
        deferred_populate_overrides: vec![],
        deferred_populate_struct_field_overrides: vec![],
        struct_field_type_overrides: vec![],
        read_shape_overrides: vec![],
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
        extra_attributes: vec![ExtraField {
            name: "retentionInDays",
            read_source: Some("retentionInDays"),
            description: Some(
                "The number of days to retain the log events in the log group. If unset, events never expire. The provider applies changes via PutRetentionPolicy / DeleteRetentionPolicy.",
            ),
        }],
        identity_overrides: vec![],
        deferred_populate_overrides: vec![],
        deferred_populate_struct_field_overrides: vec![],
        struct_field_type_overrides: vec![],
        read_shape_overrides: vec![],
        derived_attributes: vec![],
    }]
}

/// Returns SQS resource definitions.
///
/// SQS does not expose its mutable knobs as flat fields on `CreateQueue`;
/// they live inside an `Attributes: Map<QueueAttributeName, String>`.
/// We surface every `QueueAttributeName` variant as a flat top-level
/// attribute. The map↔flat packing/unpacking happens in the hand-written
/// `services/sqs/queue.rs`; the codegen here only emits the schema shape.
///
/// Variants intentionally skipped (per
/// `feedback_resource_unit_complete_attrs.md` we surface "every attribute
/// the API exposes" — these are the genuinely non-attribute entries):
/// - `All`: meta marker used by `GetQueueAttributes` to mean "fetch
///   everything". Not a queue attribute itself.
pub fn sqs_resources() -> Vec<ResourceDef> {
    vec![ResourceDef {
        name: "sqs.Queue",
        service_namespace: "com.amazonaws.sqs",
        schema_structure: None,
        simple_delete: true,
        noop_update: false,
        create_op: "CreateQueue",
        // `GetQueueAttributes` returns a Map, not a structure — the read
        // path is hand-written in `services/sqs/queue.rs`. The codegen's
        // `extract_*_attributes` emission skips this resource via the
        // `manual_methods` scan (it finds `extract_sqs_queue_attributes`).
        read_structure: None,
        read_ops: vec![],
        delete_op: "DeleteQueue",
        // Every queue attribute SetQueueAttributes accepts goes here.
        // Listing them marks them updatable on the schema side; the
        // immutable ones (FifoQueue, FifoThroughputLimit, DeduplicationScope)
        // are forced create-only via `create_only_overrides`.
        update_ops: vec![UpdateOp {
            operation: "SetQueueAttributes",
            fields: FieldLayout::Flat(vec![
                "Policy",
                "VisibilityTimeout",
                "MaximumMessageSize",
                "MessageRetentionPeriod",
                "DelaySeconds",
                "ReceiveMessageWaitTimeSeconds",
                "RedrivePolicy",
                "RedriveAllowPolicy",
                "ContentBasedDeduplication",
                "KmsMasterKeyId",
                "KmsDataKeyReusePeriodSeconds",
                "SqsManagedSseEnabled",
            ]),
        }],
        identifier: "QueueUrl",
        has_tags: true,
        type_overrides: vec![
            ("QueueArn", "super::arn()"),
            ("KmsMasterKeyId", "super::kms_key_id()"),
            ("Policy", "super::iam_policy_document()"),
            ("RedrivePolicy", "super::sqs_redrive_policy()"),
            ("RedriveAllowPolicy", "super::sqs_redrive_allow_policy()"),
            ("VisibilityTimeout", "AttributeType::int()"),
            ("MaximumMessageSize", "AttributeType::int()"),
            ("MessageRetentionPeriod", "AttributeType::int()"),
            ("DelaySeconds", "AttributeType::int()"),
            ("ReceiveMessageWaitTimeSeconds", "AttributeType::int()"),
            ("KmsDataKeyReusePeriodSeconds", "AttributeType::int()"),
            ("FifoQueue", "AttributeType::bool()"),
            ("ContentBasedDeduplication", "AttributeType::bool()"),
            ("SqsManagedSseEnabled", "AttributeType::bool()"),
            ("ApproximateNumberOfMessages", "AttributeType::int()"),
            ("ApproximateNumberOfMessagesDelayed", "AttributeType::int()"),
            (
                "ApproximateNumberOfMessagesNotVisible",
                "AttributeType::int()",
            ),
            ("CreatedTimestamp", "AttributeType::int()"),
            ("LastModifiedTimestamp", "AttributeType::int()"),
        ],
        // Keep the raw `Attributes` map out of the schema; the synthetic
        // entries below project every QueueAttributeName key.
        exclude_fields: vec!["Attributes"],
        // FifoQueue, FifoThroughputLimit, and DeduplicationScope cannot be
        // changed after the queue is created — AWS rejects SetQueueAttributes
        // for these.
        create_only_overrides: vec![
            "QueueName",
            "FifoQueue",
            "FifoThroughputLimit",
            "DeduplicationScope",
        ],
        enum_aliases: vec![],
        to_dsl_overrides: vec![],
        required_overrides: vec![],
        extra_read_only: vec![],
        // QueueArn + the runtime counters and timestamps are read-only.
        read_only_overrides: vec![
            "QueueArn",
            "ApproximateNumberOfMessages",
            "ApproximateNumberOfMessagesDelayed",
            "ApproximateNumberOfMessagesNotVisible",
            "CreatedTimestamp",
            "LastModifiedTimestamp",
        ],
        extra_attributes: vec![
            // Writable / mutable knobs
            ExtraField {
                name: "Policy",
                read_source: None,
                description: Some(
                    "The queue's access policy, as a JSON-encoded IAM policy document.",
                ),
            },
            ExtraField {
                name: "VisibilityTimeout",
                read_source: None,
                description: Some(
                    "The visibility timeout for the queue, in seconds. Defaults to 30. Range 0–43,200.",
                ),
            },
            ExtraField {
                name: "MessageRetentionPeriod",
                read_source: None,
                description: Some(
                    "The length of time, in seconds, for which Amazon SQS retains a message. Defaults to 345,600 (4 days). Range 60–1,209,600 (1 minute–14 days).",
                ),
            },
            ExtraField {
                name: "DelaySeconds",
                read_source: None,
                description: Some(
                    "The length of time, in seconds, for which the delivery of all messages in the queue is delayed. Defaults to 0. Range 0–900 (15 minutes).",
                ),
            },
            ExtraField {
                name: "MaximumMessageSize",
                read_source: None,
                description: Some(
                    "The limit of how many bytes a message can contain before Amazon SQS rejects it. Defaults to 262,144 (256 KiB). Range 1,024–262,144 (1–256 KiB).",
                ),
            },
            ExtraField {
                name: "ReceiveMessageWaitTimeSeconds",
                read_source: None,
                description: Some(
                    "The duration, in seconds, for which a ReceiveMessage call waits for a message to arrive (long polling). Defaults to 0. Range 0–20.",
                ),
            },
            ExtraField {
                name: "RedrivePolicy",
                read_source: None,
                description: Some(
                    "Dead-letter queue redrive policy, as a JSON document with `deadLetterTargetArn` and `maxReceiveCount` keys.",
                ),
            },
            ExtraField {
                name: "RedriveAllowPolicy",
                read_source: None,
                description: Some(
                    "Permissions for source queues that can use this queue as their dead-letter destination, as a JSON document.",
                ),
            },
            // FIFO-specific (all create-only except ContentBasedDeduplication)
            ExtraField {
                name: "FifoQueue",
                read_source: None,
                description: Some(
                    "Whether the queue is FIFO. Create-only. The queue name must end with `.fifo` when true.",
                ),
            },
            ExtraField {
                name: "ContentBasedDeduplication",
                read_source: None,
                description: Some(
                    "FIFO only: enables content-based message deduplication using a SHA-256 hash of the message body.",
                ),
            },
            ExtraField {
                name: "DeduplicationScope",
                read_source: None,
                description: Some(
                    "FIFO only, create-only: scope of message deduplication. Valid values: `messageGroup`, `queue`.",
                ),
            },
            ExtraField {
                name: "FifoThroughputLimit",
                read_source: None,
                description: Some(
                    "FIFO only, create-only: per-queue or per-message-group throughput limit. Valid values: `perQueue`, `perMessageGroupId`.",
                ),
            },
            // KMS encryption
            ExtraField {
                name: "KmsMasterKeyId",
                read_source: None,
                description: Some(
                    "The ID of an AWS KMS customer master key (CMK) to use for server-side encryption (SSE-KMS). Use `alias/aws/sqs` for the AWS-managed key.",
                ),
            },
            ExtraField {
                name: "KmsDataKeyReusePeriodSeconds",
                read_source: None,
                description: Some(
                    "Length of time, in seconds, that SQS can reuse a data key before invoking KMS again. Defaults to 300 (5 minutes). Range 60–86,400.",
                ),
            },
            ExtraField {
                name: "SqsManagedSseEnabled",
                read_source: None,
                description: Some(
                    "Enables SQS-managed server-side encryption (SSE-SQS). Mutually exclusive with KmsMasterKeyId.",
                ),
            },
            // Read-only
            ExtraField {
                name: "QueueArn",
                read_source: None,
                description: Some("The Amazon Resource Name (ARN) of the queue."),
            },
            ExtraField {
                name: "ApproximateNumberOfMessages",
                read_source: None,
                description: Some(
                    "Approximate number of visible messages in the queue. Runtime metric reported by the SQS service.",
                ),
            },
            ExtraField {
                name: "ApproximateNumberOfMessagesDelayed",
                read_source: None,
                description: Some(
                    "Approximate number of messages currently being delayed before becoming visible. Runtime metric.",
                ),
            },
            ExtraField {
                name: "ApproximateNumberOfMessagesNotVisible",
                read_source: None,
                description: Some(
                    "Approximate number of messages that are in flight (received but not yet deleted or returned to the queue). Runtime metric.",
                ),
            },
            ExtraField {
                name: "CreatedTimestamp",
                read_source: None,
                description: Some("Unix epoch seconds at which the queue was created."),
            },
            ExtraField {
                name: "LastModifiedTimestamp",
                read_source: None,
                description: Some(
                    "Unix epoch seconds at which the queue's attributes were last modified.",
                ),
            },
        ],
        identity_overrides: vec![],
        deferred_populate_overrides: vec![],
        deferred_populate_struct_field_overrides: vec![],
        struct_field_type_overrides: vec![],
        read_shape_overrides: vec![],
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
            type_code: "AttributeType::string()",
        };
        assert_eq!(o.name, "arn");
        assert!(o.provider_name.is_none());
        assert_eq!(o.type_code, "AttributeType::string()");
    }

    #[test]
    fn sts_caller_identity_declares_outputs_explicitly() {
        let defs = sts_data_sources();
        let ds = &defs[0];
        let outs = ds.shape.single_outputs().expect("Single shape");
        let names: Vec<&str> = outs.iter().map(|o| o.name).collect();
        assert_eq!(names, vec!["account_id", "arn", "user_id"]);
        assert_eq!(outs[0].provider_name, Some("Account"));
        assert_eq!(outs[0].type_code, "super::aws_account_id()");
        assert_eq!(outs[1].type_code, "super::arn()");
    }

    #[test]
    fn identitystore_user_declares_outputs_explicitly() {
        let defs = identitystore_data_sources();
        let ds = &defs[0];
        let outs = ds.shape.single_outputs().expect("Single shape");
        let names: Vec<&str> = outs.iter().map(|o| o.name).collect();
        assert_eq!(names, vec!["display_name", "emails"]);
        assert_eq!(outs[0].provider_name, Some("DisplayName"));
        assert_eq!(outs[1].provider_name, Some("Emails"));
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
        let outs = ds.shape.single_outputs().expect("Single shape");
        let outputs: Vec<&str> = outs.iter().map(|o| o.name).collect();
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
        let arn = outs.iter().find(|o| o.name == "arn").unwrap();
        assert!(arn.provider_name.is_none());
        let region = outs.iter().find(|o| o.name == "region").unwrap();
        assert_eq!(region.provider_name, Some("LocationConstraint"));
    }

    #[test]
    fn iam_roles_is_list_aggregated_with_path_and_regex_inputs() {
        let defs = iam_data_sources();
        assert_eq!(defs.len(), 1);
        let ds = &defs[0];
        assert_eq!(ds.name, "iam.Roles");

        let inputs: Vec<&str> = ds.inputs.iter().map(|i| i.name).collect();
        assert_eq!(inputs, vec!["path_prefix", "name_regex"]);
        for i in &ds.inputs {
            assert!(!i.required, "iam.Roles inputs are all optional");
        }

        let DataSourceShape::ListAggregated { aggregated_outputs } = &ds.shape else {
            panic!("expected ListAggregated, got Single");
        };
        let names: Vec<&str> = aggregated_outputs.iter().map(|o| o.name).collect();
        assert_eq!(names, vec!["arns", "names"]);
        assert_eq!(aggregated_outputs[0].item_type, "super::iam_role_arn()");
        assert_eq!(aggregated_outputs[1].item_type, "AttributeType::string()");
    }

    #[test]
    fn data_source_def_carries_output_attributes() {
        let def = DataSourceDef {
            name: "test.X",
            service_namespace: "com.test",
            inputs: vec![],
            shape: DataSourceShape::Single {
                output_attributes: vec![DataSourceOutput {
                    name: "arn",
                    provider_name: None,
                    description: "",
                    type_code: "AttributeType::string()",
                }],
            },
        };
        let outs = def.shape.single_outputs().expect("Single shape");
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].name, "arn");
    }
}
