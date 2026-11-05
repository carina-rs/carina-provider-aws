use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;

use carina_core::protocol_convert::{
    attribute_type_to_protocol, protocol_to_resource_schema, resource_schema_to_protocol,
};
use carina_core::provider::{
    BoxFuture, CanonicalProviderConfigAttributes, InitializeOutcomeMint, LoadedProviderSnapshot,
    ProviderFactory, ProviderInitialization, ProviderReadyConfig, ProviderResult,
};
use carina_core::schema::{
    RawShape, SchemaRegistry, ValidatedConfigSchema, ValidatedResourceSchema, types,
};
use carina_provider_aws::AwsProviderFactory;
use serde::Deserialize;

#[derive(Deserialize)]
struct BaselineInventory {
    revision: String,
    attributes: Vec<CidrAttribute>,
}

#[derive(Deserialize)]
struct CidrAttribute {
    resource: String,
    path: String,
    kind: String,
    schema: String,
    line: usize,
}

fn inventory() -> BaselineInventory {
    // Extracted from every generated schema on origin/main, before this slice.
    serde_json::from_str(include_str!("fixtures/ec2_cidr_origin_main.json")).unwrap()
}

fn wire_type(ty: carina_core::schema::AttributeType) -> serde_json::Value {
    let schema = carina_core::schema::ResourceSchema::new("test.CidrDeclaration")
        .attribute(carina_core::schema::AttributeSchema::optional("cidr", ty).build())
        .validate()
        .unwrap();
    serde_json::to_value(
        attribute_type_to_protocol(schema.effective_attribute("cidr").unwrap().field_type())
            .unwrap(),
    )
    .unwrap()
}

fn replace_types(
    value: &mut serde_json::Value,
    replacements: &[(serde_json::Value, serde_json::Value)],
) {
    if let Some((_, replacement)) = replacements.iter().find(|(original, _)| original == value) {
        *value = replacement.clone();
        return;
    }
    match value {
        serde_json::Value::Object(fields) => {
            for value in fields.values_mut() {
                replace_types(value, replacements);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                replace_types(value, replacements);
            }
        }
        _ => {}
    }
}

/// Restore only the four CIDR declarations changed by this slice, retaining
/// the full real schemas around them. Protocol decoding restores the built-in
/// CIDR validators; the enumerated graph below guards this baseline projection.
struct OriginCidrFactory;

impl ProviderFactory for OriginCidrFactory {
    fn name(&self) -> &str {
        "aws"
    }
    fn display_name(&self) -> &str {
        "origin/main CIDR schemas"
    }
    fn provider_config_attribute_types(&self) -> ValidatedConfigSchema {
        AwsProviderFactory.provider_config_attribute_types()
    }
    fn validate_config(&self, attrs: &CanonicalProviderConfigAttributes) -> ProviderResult<()> {
        AwsProviderFactory.validate_config(attrs)
    }
    fn create_provider(
        &self,
        _: Option<&str>,
        _: &ProviderReadyConfig,
        _: InitializeOutcomeMint,
    ) -> BoxFuture<'_, ProviderResult<ProviderInitialization>> {
        panic!("CIDR validation tests must not create an AWS client")
    }
    fn schemas(&self) -> Vec<ValidatedResourceSchema> {
        let replacements = [
            (carina_aws_types::ec2_vpc_cidr_block(), types::ipv4_cidr()),
            (
                carina_aws_types::ec2_subnet_ipv4_cidr_block(),
                types::ipv4_cidr(),
            ),
            (
                carina_aws_types::ec2_subnet_ipv6_cidr_block(),
                types::ipv6_cidr(),
            ),
            (
                carina_aws_types::ec2_transit_gateway_cidr_block(),
                types::ipv4_cidr(),
            ),
        ]
        .into_iter()
        .map(|(original, replacement)| (wire_type(original), wire_type(replacement)))
        .collect::<Vec<_>>();
        AwsProviderFactory
            .schemas()
            .into_iter()
            .map(|schema| {
                let mut wire =
                    serde_json::to_value(resource_schema_to_protocol(&schema).unwrap()).unwrap();
                replace_types(&mut wire, &replacements);
                protocol_to_resource_schema(&serde_json::from_value(wire).unwrap()).unwrap()
            })
            .collect()
    }
    fn schema_witness(&self) -> carina_core::schema::ValidatedProviderSchemaWitness {
        carina_core::schema::ValidatedProviderSchemaWitness::new(
            self.schemas(),
            self.provider_config_attribute_types(),
            Default::default(),
            vec![],
        )
        .unwrap()
    }
}

fn loaded(factory: impl ProviderFactory + 'static) -> LoadedProviderSnapshot {
    carina_core::provider::loaded_provider_snapshot_from_factories(
        Arc::new(vec![Box::new(factory)]),
        SchemaRegistry::new(),
        carina_core::EnvironmentSnapshot::empty(),
    )
}

fn validate(snapshot: &LoadedProviderSnapshot, source: &str) -> Result<(), String> {
    let context = snapshot.provider_context();
    let parsed =
        carina_core::parser::parse_file(source, Path::new("ec2-cidr-references.crn"), context)
            .map_err(|error| error.to_string())?;
    carina_core::validation::validate_resources(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::from(["aws".to_string()]),
        context,
    )?;
    let bindings = carina_core::binding_index::BindingIndex::from_parsed(
        &parsed,
        snapshot.schema_registry(),
        context,
    );
    carina_core::validation::validate_resource_ref_types(
        &parsed,
        snapshot.schema_registry(),
        &HashSet::new(),
        &bindings,
    )
    .map_err(|error| error.to_string())
}

fn resource(attribute: &CidrAttribute, expression: &str) -> String {
    match attribute.resource.as_str() {
        "ec2.Vpc" => format!("aws.ec2.Vpc {{ cidr_block = {expression} }}"),
        "ec2.Subnet" => {
            let ipv4 = if attribute.path == "ipv6_cidr_block" {
                "cidr_block = '10.0.0.0/24'"
            } else {
                ""
            };
            format!(
                "aws.ec2.Subnet {{ vpc_id = 'vpc-12345678' {ipv4} {} = {expression} }}",
                attribute.path
            )
        }
        "ec2.TransitGateway" => format!(
            "aws.ec2.TransitGateway {{ options = {{ transit_gateway_cidr_blocks = [{expression}] }} }}"
        ),
        "ec2.Route" => format!(
            "aws.ec2.Route {{ route_table_id = 'rtb-12345678' gateway_id = 'igw-12345678' destination_cidr_block = {expression} }}"
        ),
        "ec2.SecurityGroupIngress" | "ec2.SecurityGroupEgress" => format!(
            "aws.{} {{ group_id = 'sg-12345678' ip_protocol = all {} = {expression} }}",
            attribute.resource, attribute.path
        ),
        other => panic!("missing realistic CIDR resource shape: {other}"),
    }
}

fn literal(kind: &str) -> &'static str {
    match kind {
        "Ipv4Cidr" => "'10.0.0.0/24'",
        "Ipv6Cidr" => "'2001:db8::/64'",
        _ => panic!("unexpected CIDR kind"),
    }
}

#[test]
fn base_cidr_identities_are_bare_and_core_rejects_edges_from_them() {
    use carina_core::schema::{AssignabilityEdge, ProviderSchemaValidationError, TypeIdentity};

    for (base, strict) in [
        (
            types::ipv4_cidr(),
            carina_aws_types::AwsType::Ec2VpcCidrBlock,
        ),
        (
            types::ipv4_cidr(),
            carina_aws_types::AwsType::Ec2SubnetIpv4CidrBlock,
        ),
        (
            types::ipv6_cidr(),
            carina_aws_types::AwsType::Ec2SubnetIpv6CidrBlock,
        ),
    ] {
        let RawShape::String {
            identity: Some(identity),
            ..
        } = base.raw_shape()
        else {
            panic!("core CIDRs must be identified strings")
        };
        assert_eq!(*identity, TypeIdentity::bare(strict.identity().kind));
        let error = carina_core::schema::ValidatedProviderSchemaWitness::new(
            AwsProviderFactory.schemas(),
            AwsProviderFactory.provider_config_attribute_types(),
            Default::default(),
            vec![AssignabilityEdge::new(identity.clone(), strict.identity()).unwrap()],
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                ProviderSchemaValidationError::BuiltinAssignabilityEndpoint { .. }
            ),
            "{error}"
        );
    }
}

#[test]
fn vpc_to_subnet_edge_has_value_containment_without_the_reverse() {
    use carina_core::resource::{ConcreteValue, Value};
    let schemas = AwsProviderFactory.schemas();
    let attr = |resource| {
        schemas
            .iter()
            .find(|schema| schema.as_schema().resource_type() == resource)
            .unwrap()
            .effective_attribute("cidr_block")
            .unwrap()
    };
    let vpc = attr("ec2.Vpc");
    let subnet = attr("ec2.Subnet");
    // The VPC validator calls the same Subnet parser/prefix check before
    // excluding ranges. Preserve noncanonical host bits at every prefix.
    for prefix in 16..=28 {
        let value: Value = Value::Concrete(ConcreteValue::String(
            format!("100.68.0.18/{prefix}").into(),
        ));
        assert!(vpc.validate(&value).is_ok());
        assert!(subnet.validate(&value).is_ok());
    }
    let value: Value = Value::Concrete(ConcreteValue::String("127.0.0.0/24".into()));
    assert!(subnet.validate(&value).is_ok());
    assert!(vpc.validate(&value).is_err());
}

#[test]
fn origin_main_cidr_inventory_enumerates_every_generated_cidr_attribute() {
    let inventory = inventory();
    assert_eq!(
        inventory.revision,
        "cee78ef984897dfdfcee09f409245bff94e2d73c"
    );
    let expected = inventory
        .attributes
        .iter()
        .map(|attr| {
            assert!(
                attr.schema
                    .starts_with("carina-provider-aws/src/schemas/generated/")
            );
            assert!(attr.line > 0);
            (attr.resource.clone(), attr.path.clone(), attr.kind.clone())
        })
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    fn walk(
        resource: &str,
        path: &str,
        ty: &carina_core::schema::CompiledAttributeType,
        out: &mut BTreeSet<(String, String, String)>,
    ) {
        match ty.raw_shape() {
            RawShape::String {
                identity: Some(identity),
                ..
            } if identity.provider.is_none()
                && identity.segments.is_empty()
                && matches!(identity.kind.as_str(), "Ipv4Cidr" | "Ipv6Cidr") =>
            {
                out.insert((resource.into(), path.into(), identity.kind.clone()));
            }
            RawShape::List { element_type, .. } => {
                walk(resource, &format!("{path}[]"), element_type, out)
            }
            RawShape::Struct { fields, .. } => {
                for field in fields {
                    walk(
                        resource,
                        &format!("{path}.{}", field.parts().name),
                        field.parts().field_type,
                        out,
                    );
                }
            }
            RawShape::Union(members) => {
                for member in members {
                    walk(resource, path, member, out);
                }
            }
            RawShape::Map { value, .. } => walk(resource, &format!("{path}.*"), value, out),
            _ => {}
        }
    }
    for schema in OriginCidrFactory.schemas() {
        for attr in schema.effective_attributes() {
            walk(
                schema.as_schema().resource_type(),
                attr.name(),
                attr.field_type(),
                &mut actual,
            );
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn resource_cidrs_widen_to_base_sinks_and_vpc_flows_into_subnet() {
    let inventory = inventory();
    let current = loaded(AwsProviderFactory);
    for (source_resource, source_path, sink_resource, sink_path) in [
        ("ec2.Vpc", "cidr_block", "ec2.Subnet", "cidr_block"),
        (
            "ec2.Vpc",
            "cidr_block",
            "ec2.Route",
            "destination_cidr_block",
        ),
        (
            "ec2.Vpc",
            "cidr_block",
            "ec2.SecurityGroupIngress",
            "cidr_ip",
        ),
        (
            "ec2.Vpc",
            "cidr_block",
            "ec2.SecurityGroupEgress",
            "cidr_ip",
        ),
        (
            "ec2.Subnet",
            "cidr_block",
            "ec2.Route",
            "destination_cidr_block",
        ),
        (
            "ec2.Subnet",
            "cidr_block",
            "ec2.SecurityGroupIngress",
            "cidr_ip",
        ),
        (
            "ec2.Subnet",
            "cidr_block",
            "ec2.SecurityGroupEgress",
            "cidr_ip",
        ),
        (
            "ec2.Subnet",
            "ipv6_cidr_block",
            "ec2.SecurityGroupIngress",
            "cidr_ipv6",
        ),
        (
            "ec2.Subnet",
            "ipv6_cidr_block",
            "ec2.SecurityGroupEgress",
            "cidr_ipv6",
        ),
    ] {
        let find = |resource: &str, path: &str| {
            inventory
                .attributes
                .iter()
                .find(|attr| attr.resource == resource && attr.path == path)
                .unwrap()
        };
        let source = find(source_resource, source_path);
        let sink = find(sink_resource, sink_path);
        let configuration = format!(
            "let source = {}\n{}\n",
            resource(source, literal(&source.kind)),
            resource(sink, &format!("source.{source_path}"))
        );
        validate(&current, &configuration).unwrap_or_else(|error| {
            panic!("{source_resource}.{source_path} -> {sink_resource}.{sink_path}: {error}")
        });
    }
}

#[derive(Clone, Copy, Debug)]
enum CidrFlowExpectation {
    Accepted,
    // (a) Generic CIDRs and Subnet IPv4 CIDRs do not prove the stricter
    // sink's prefix/excluded-range rules; no narrowing edge is justified.
    StricterSink,
    // (b) The corrected TransitGateway domain includes IPv6, so its output
    // cannot prove an IPv4-only sink's family (or its narrower prefix rules).
    DualFamily,
    // (c) VPC/Subnet IPv4 CIDRs include /25..=/28; TransitGateway requires
    // /24 or larger. Resource provenance cannot establish containment.
    TransitPrefix,
}

fn check_flow_matrix<const N: usize>(
    inventory: &BaselineInventory,
    baseline: &LoadedProviderSnapshot,
    current: &LoadedProviderSnapshot,
    sites: [(&str, &str); N],
    matrix: [[CidrFlowExpectation; N]; N],
) -> [usize; 4] {
    let attributes = sites.map(|(resource, path)| {
        inventory
            .attributes
            .iter()
            .find(|attribute| attribute.resource == resource && attribute.path == path)
            .unwrap_or_else(|| panic!("missing origin/main CIDR attribute {resource}.{path}"))
    });
    let mut counts = [0; 4];
    for (source, row) in attributes.iter().zip(matrix) {
        for (sink, expected) in attributes.iter().zip(row) {
            let flow = format!(
                "{}.{} -> {}.{}",
                source.resource, source.path, sink.resource, sink.path
            );
            let configuration = format!(
                "let source = {}\n{}\n",
                resource(source, literal(&source.kind)),
                resource(
                    sink,
                    &format!("source.{}", source.path.replace("[]", "[0]"))
                )
            );
            validate(baseline, &configuration)
                .unwrap_or_else(|error| panic!("origin/main {flow}: {error}"));
            match (expected, validate(current, &configuration)) {
                (CidrFlowExpectation::Accepted, Ok(())) => {}
                (CidrFlowExpectation::Accepted, Err(error)) => panic!("{flow}: {error}"),
                (reason, Ok(())) => panic!("{flow}: expected rejection for {reason:?}"),
                (reason, Err(error)) => assert!(
                    error.contains("cannot assign"),
                    "{flow}: expected a reference type rejection for {reason:?}, got {error}"
                ),
            }
            counts[expected as usize] += 1;
        }
    }
    counts
}

#[test]
fn cidr_reference_flow_matrix_matches_containment_with_the_loaded_provider() {
    use CidrFlowExpectation::{
        Accepted as A, DualFamily as D, StricterSink as S, TransitPrefix as T,
    };

    let inventory = inventory();
    let baseline = loaded(OriginCidrFactory);
    let current = loaded(AwsProviderFactory);
    // Each matrix uses the same order for source rows and sink columns.
    let ipv4_sites = [
        ("ec2.Route", "destination_cidr_block"),
        ("ec2.SecurityGroupEgress", "cidr_ip"),
        ("ec2.SecurityGroupIngress", "cidr_ip"),
        ("ec2.Subnet", "cidr_block"),
        (
            "ec2.TransitGateway",
            "options.transit_gateway_cidr_blocks[]",
        ),
        ("ec2.Vpc", "cidr_block"),
    ];
    let ipv4_matrix = [
        [A, A, A, S, S, S], // Route
        [A, A, A, S, S, S], // SecurityGroupEgress
        [A, A, A, S, S, S], // SecurityGroupIngress
        [A, A, A, A, T, S], // Subnet
        [D, D, D, D, A, D], // TransitGateway
        [A, A, A, A, T, A], // Vpc
    ];
    let ipv6_sites = [
        ("ec2.SecurityGroupEgress", "cidr_ipv6"),
        ("ec2.SecurityGroupIngress", "cidr_ipv6"),
        ("ec2.Subnet", "ipv6_cidr_block"),
    ];
    let ipv6_matrix = [
        [A, A, S], // SecurityGroupEgress
        [A, A, S], // SecurityGroupIngress
        [A, A, A], // Subnet
    ];
    let matrix_sites = ipv4_sites
        .into_iter()
        .chain(ipv6_sites)
        .collect::<BTreeSet<_>>();
    let inventory_sites = inventory
        .attributes
        .iter()
        .map(|attribute| (attribute.resource.as_str(), attribute.path.as_str()))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        matrix_sites, inventory_sites,
        "every inventoried CIDR attribute needs a matrix row and column"
    );

    let ipv4 = check_flow_matrix(&inventory, &baseline, &current, ipv4_sites, ipv4_matrix);
    let ipv6 = check_flow_matrix(&inventory, &baseline, &current, ipv6_sites, ipv6_matrix);
    let counts: [usize; 4] = std::array::from_fn(|index| ipv4[index] + ipv6[index]);
    assert_eq!(
        counts,
        [26, 12, 5, 2],
        "45 origin/main flows: accepted, stricter sink, dual-family source, TransitGateway prefix"
    );
}
