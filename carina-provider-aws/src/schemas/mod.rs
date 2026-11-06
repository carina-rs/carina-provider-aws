//! AWS resource schema definitions

pub mod config;
pub mod generated;

use carina_core::provider::ProviderFactory;
use carina_core::schema::{
    AssignabilityEdge, SchemaRegistry, ValidatedProviderSchemaWitness, ValidatedResourceSchema,
};

/// Returns all AWS schemas
pub fn all_schemas() -> Vec<ValidatedResourceSchema> {
    generated::configs()
        .into_iter()
        .map(|config| {
            config
                .schema
                .validate()
                .unwrap_or_else(|error| panic!("{error}"))
        })
        .collect()
}

/// Return the provider-wide identity edges shared by native and WASM schemas.
pub fn assignability_edges() -> Vec<AssignabilityEdge> {
    // Every VPC IPv4 CIDR satisfies the Subnet IPv4 CIDR validator: both
    // preserve core's parser and require /16..=/28, while VPC alone excludes
    // reserved ranges. The reverse direction does not have containment.
    vec![
        AssignabilityEdge::new(
            carina_aws_types::AwsType::Ec2VpcCidrBlock.identity(),
            carina_aws_types::AwsType::Ec2SubnetIpv4CidrBlock.identity(),
        )
        .expect("VPC and Subnet CIDR identities must be distinct"),
    ]
}

/// Return the provider's one aggregate schema witness. Reference forms are
/// intentionally empty until AWS name-or-ARN forms are designed separately.
pub fn provider_schema_witness() -> ValidatedProviderSchemaWitness {
    ValidatedProviderSchemaWitness::new(
        all_schemas(),
        crate::AwsProviderFactory.provider_config_attribute_types(),
        Default::default(),
        assignability_edges(),
    )
    .expect("AWS provider schemas must form one valid aggregate witness")
}

/// Build a registry from the aggregate witness rather than admitting schemas
/// one by one without their provider-wide declarations.
pub fn schema_registry() -> SchemaRegistry {
    let witness = provider_schema_witness();
    let mut registry = SchemaRegistry::new();
    registry
        .insert_provider_witness("aws", &witness)
        .expect("AWS provider witness must populate its schema registry");
    registry
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use carina_aws_types::AwsType;
    use carina_core::differ::Diff;
    use carina_core::executor::normalized::apply_desired_normalization;
    use carina_core::provider::normalize_provider_states;
    use carina_core::resource::{ConcreteValue, Current, ResolvedResource, Resource, Value};
    use carina_core::schema::{
        AttributeType, IdentityMode, PatternAttributeType, PatternForm, PatternResourceSchema,
        RawShape, ResourceSchema, SchemaKind, SchemaRegistry, Shape, ShapeWalkBudget, TypeInSchema,
        UniqueNameSpec, ValidatedConfigSchema,
    };
    use indexmap::IndexMap;

    use crate::AwsNormalizer;

    fn loaded_aws() -> carina_core::provider::LoadedProviderSnapshot {
        carina_core::provider::loaded_provider_snapshot_from_factories(
            std::sync::Arc::new(vec![Box::new(crate::AwsProviderFactory)]),
            SchemaRegistry::new(),
            carina_core::EnvironmentSnapshot::empty(),
        )
    }

    fn validate_named_record_type_annotation(declaration: &str) -> Result<(), String> {
        let source = format!(
            "{declaration}\n\
             aws.route53.RecordSet {{\n\
               hosted_zone_id = 'Z1234567890'\n\
               name = 'validation.example.com'\n\
               type = record_type\n\
               ttl = 300\n\
               resource_records = ['target.example.com']\n\
             }}"
        );
        let snapshot = loaded_aws();
        let context = snapshot.provider_context();
        let parsed = carina_core::parser::parse_file(
            &source,
            std::path::Path::new("named-record-type.crn"),
            context,
        )
        .map_err(|error| error.to_string())?;
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &std::collections::HashSet::from(["aws".to_string()]),
            context,
        )
    }

    #[test]
    fn route53_record_type_function_annotation_rejects_string_literals() {
        let error = validate_named_record_type_annotation(
            "fn f(t: aws.route53.RecordSet.Type) { t }\nlet record_type = f('CNAME')",
        )
        .expect_err("named enum parameters require an enum identifier");
        assert!(error.contains("expects an enum identifier"), "{error}");
    }

    #[test]
    fn route53_record_type_function_annotation_accepts_cname_identifier() {
        validate_named_record_type_annotation(
            "fn f(t: aws.route53.RecordSet.Type) { t }\n\
             let record_type = f(aws.route53.RecordSet.Type.cname)",
        )
        .expect("a Route 53 named enum parameter must accept its CNAME identifier");
    }

    #[test]
    fn loaded_catalog_has_one_enum_definition_per_identity() {
        let snapshot = loaded_aws();
        let context = snapshot.provider_context();
        let mut enum_count = 0;
        let mut conflicts = Vec::new();
        for owned in context.known_custom_types() {
            match context.enum_declaration(owned) {
                Ok(Some(_)) => enum_count += 1,
                Ok(None) => {}
                Err(error) => conflicts.push(error),
            }
        }
        assert!(enum_count > 0, "the loaded AWS catalog must contain enums");
        assert!(
            conflicts.is_empty(),
            "each identity must have one enum definition across all loaded AWS schemas: {conflicts:#?}"
        );
    }

    #[test]
    fn acm_certificate_dns_example_validates_with_distinct_record_type_identities() {
        let snapshot = loaded_aws();
        let context = snapshot.provider_context();
        let parsed = carina_core::parser::parse_file(
            include_str!("../../../examples/acm_certificate/main.crn"),
            std::path::Path::new("examples/acm_certificate/main.crn"),
            context,
        )
        .expect("parse ACM DNS validation example");
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &std::collections::HashSet::from(["aws".to_string()]),
            context,
        )
        .expect("ACM DNS validation example must validate with distinct enum identities");
    }

    #[test]
    fn assignability_edges_loaded_catalog_rejects_unrelated_named_identities() {
        use carina_core::schema::TypeAssignability;

        let snapshot = loaded_aws();
        let context = snapshot.provider_context();
        assert!(context.custom_types.is_loaded());
        let registry = snapshot.schema_registry();
        let role = registry
            .get("aws", "iam.Role", SchemaKind::Resource)
            .unwrap();
        let subnet = registry
            .get("aws", "ec2.Subnet", SchemaKind::Resource)
            .unwrap();
        let source = role.as_schema().attributes()["role_name"].attr_type();
        let sink = subnet.as_schema().attributes()["vpc_id"].attr_type();
        for identity in ["aws.iam.Role.Name", "aws.ec2.Vpc.Id"] {
            assert!(
                context
                    .custom_types
                    .known()
                    .iter()
                    .any(|owned| owned.identity().to_string() == identity),
                "{identity} must be declared in the loaded catalog"
            );
        }
        assert_eq!(
            role.as_schema()
                .type_in_schema(source)
                .assignment_to(subnet.assignment_sink(sink, &context.custom_types)),
            TypeAssignability::Mismatch,
            "an empty edge declaration must prove a mismatch, not defer it as Unchecked"
        );
    }

    fn validate_lifecycle_filter(filter: &str) -> Result<(), String> {
        let source = format!(
            "aws.s3.BucketLifecycleConfiguration {{\n\
             bucket = 'lifecycle-test'\n\
             rules = [{{\n\
               status = aws.s3.BucketLifecycleConfiguration.LifecycleRule.Status.enabled\n\
               filter = {{{filter}}}\n\
             }}]\n\
             }}"
        );
        let snapshot = carina_core::provider::loaded_provider_snapshot_from_factories(
            std::sync::Arc::new(vec![Box::new(crate::AwsProviderFactory)]),
            SchemaRegistry::new(),
            carina_core::EnvironmentSnapshot::empty(),
        );
        let context = snapshot.provider_context();
        let parsed = carina_core::parser::parse_file(
            &source,
            std::path::Path::new("lifecycle-filter.crn"),
            context,
        )
        .expect("parse lifecycle filter fixture");
        carina_core::validation::validate_resources(
            &parsed,
            snapshot.schema_registry(),
            &std::collections::HashSet::from(["aws".to_string()]),
            context,
        )
    }

    #[test]
    fn lifecycle_filter_schema_validation_rejects_every_pair_of_conditions() {
        let conditions = [
            "prefix = 'logs/'",
            "tag = { key = 'kind' value = 'logs' }",
            "object_size_greater_than = 128",
            "object_size_less_than = 1024",
            "and = { prefix = 'logs/' object_size_greater_than = 128 }",
        ];
        for (index, first) in conditions.iter().enumerate() {
            for second in &conditions[index + 1..] {
                let filter = format!("{first}\n{second}");
                let error = validate_lifecycle_filter(&filter)
                    .expect_err("two lifecycle filter conditions must fail CLI schema validation");
                assert!(error.contains("rules[0].filter"), "{filter}: {error}");
                assert!(error.contains("at most one of"), "{filter}: {error}");
            }
        }
    }

    #[test]
    fn lifecycle_filter_schema_validation_accepts_empty_and_each_single_condition() {
        for filter in [
            "",
            "prefix = ''",
            "tag = { key = 'kind' value = 'logs' }",
            "object_size_greater_than = 128",
            "object_size_less_than = 1024",
            "and = { prefix = 'logs/' object_size_greater_than = 128 }",
        ] {
            validate_lifecycle_filter(filter).unwrap_or_else(|error| panic!("{filter}: {error}"));
        }
    }

    #[test]
    fn final_registry_classifies_every_kms_string_including_handwritten_structs() {
        use carina_codegen_aws::kms_references::{
            CLASSIFICATIONS, Classification, NotReference, PathSegment, classify_string,
            property_path,
        };
        use carina_core::schema::{CompiledAttributeType, CompiledResourceSchema};
        use std::collections::BTreeSet;

        #[derive(Default)]
        struct Coverage {
            references: BTreeSet<(String, String)>,
            paths: BTreeSet<(String, String)>,
        }

        fn walk(
            schema: &CompiledResourceSchema,
            ty: &CompiledAttributeType,
            path: &str,
            name: &str,
            description: Option<&str>,
            coverage: &mut Coverage,
            depth: usize,
        ) {
            assert!(depth < 64, "schema walk exceeded depth at {path}");
            coverage
                .paths
                .insert((schema.resource_type().to_string(), path.to_string()));
            let mut check =
                |pattern: Option<&str>, identity: Option<&carina_core::schema::TypeIdentity>| {
                    let classification =
                        classify_string(schema.resource_type(), path, name, pattern, description)
                            .unwrap();
                    match classification {
                        Some(Classification::Reference { arn_only }) => {
                            assert_eq!(
                                identity,
                                Some(&AwsType::KmsKeyReference.identity()),
                                "{}.{} must be a reference leaf",
                                schema.resource_type(),
                                path
                            );
                            let expected_pattern = if arn_only {
                                carina_aws_types::kms_key_arn_pattern()
                            } else {
                                carina_aws_types::kms_key_reference_pattern()
                            };
                            assert_eq!(
                                pattern,
                                Some(expected_pattern),
                                "{}.{} must use the pattern for arn_only={arn_only}",
                                schema.resource_type(),
                                path
                            );
                            coverage
                                .references
                                .insert((schema.resource_type().to_string(), path.to_string()));
                        }
                        Some(Classification::NotReference(NotReference::Absent)) => {
                            panic!(
                                "absent classification exists in registry: {}.{path}",
                                schema.resource_type()
                            );
                        }
                        Some(Classification::NotReference(NotReference::Other(reason))) => {
                            assert!(!reason.is_empty());
                            assert!(identity.is_none());
                        }
                        None => assert!(
                            identity.is_none(),
                            "unclassified reference {}.{path}",
                            schema.resource_type()
                        ),
                    }
                };
            match schema.shape_of(ty) {
                Shape::Reference {
                    identity, pattern, ..
                } => check(pattern, Some(identity)),
                Shape::String { pattern, .. } => check(pattern, None),
                Shape::UnloadedSchemaType(unloaded) => panic!(
                    "KMS coverage requires a loaded schema type for {}",
                    unloaded.identity(),
                ),
                Shape::Enum { base, .. } => {
                    walk(schema, base, path, name, description, coverage, depth + 1)
                }
                Shape::List { element_type, .. } => walk(
                    schema,
                    element_type,
                    &property_path(path, PathSegment::ListElement),
                    name,
                    description,
                    coverage,
                    depth + 1,
                ),
                Shape::Map { value, .. } => {
                    walk(
                        schema,
                        value,
                        &property_path(path, PathSegment::MapValue),
                        name,
                        description,
                        coverage,
                        depth + 1,
                    );
                }
                Shape::Struct { .. } => {
                    let mut budget = ShapeWalkBudget::new(64);
                    for field in schema
                        .struct_fields_with_budget(ty, &mut budget)
                        .unwrap()
                        .iter()
                    {
                        walk(
                            schema,
                            field.field_type(),
                            &property_path(path, PathSegment::Field(field.name())),
                            field.name(),
                            field.description(),
                            coverage,
                            depth + 1,
                        );
                    }
                }
                Shape::Union => {
                    let mut budget = ShapeWalkBudget::new(64);
                    for member in schema
                        .union_members_with_budget(ty, &mut budget)
                        .unwrap()
                        .iter()
                        .flatten()
                    {
                        walk(
                            schema,
                            member.as_attr(),
                            path,
                            name,
                            description,
                            coverage,
                            depth + 1,
                        );
                    }
                }
                Shape::Bool | Shape::Int { .. } | Shape::Float { .. } | Shape::Duration => {}
            }
        }

        let registry = super::schema_registry();
        let mut coverage = Coverage::default();
        for (_, _, _, schema) in registry.iter() {
            let schema = schema.as_schema();
            for (name, attribute) in schema.attributes() {
                walk(
                    schema,
                    attribute.attr_type(),
                    name,
                    name,
                    attribute.description(),
                    &mut coverage,
                    0,
                );
            }
        }
        for entry in CLASSIFICATIONS {
            if entry.classification == Classification::NotReference(NotReference::Absent) {
                assert!(
                    !coverage
                        .paths
                        .contains(&(entry.resource.to_string(), entry.path.to_string())),
                    "absent classification exists in registry: {}.{}",
                    entry.resource,
                    entry.path,
                );
            }
        }
        let expected = CLASSIFICATIONS
            .iter()
            .filter(|entry| matches!(entry.classification, Classification::Reference { .. }))
            .map(|entry| (entry.resource.to_string(), entry.path.to_string()))
            .collect();
        assert_eq!(
            coverage.references, expected,
            "every classified reference must be present in the final registry"
        );
    }

    #[test]
    fn acm_read_back_budget_is_300_seconds_and_other_resources_use_default() {
        let schemas = super::all_schemas();
        let certificate = schemas
            .iter()
            .map(|schema| schema.as_schema())
            .find(|schema| schema.resource_type() == "acm.Certificate")
            .unwrap();
        assert_eq!(
            certificate
                .operation_config()
                .and_then(|config| config.read_back)
                .unwrap()
                .timeout_secs()
                .get(),
            300
        );
        for schema in schemas
            .iter()
            .map(|schema| schema.as_schema())
            .filter(|schema| schema.resource_type() != "acm.Certificate")
        {
            assert!(
                schema
                    .operation_config()
                    .and_then(|config| config.read_back)
                    .is_none(),
                "{} must use the default read-back budget",
                schema.resource_type()
            );
        }
        assert_eq!(
            carina_provider_protocol::DEFAULT_READ_BACK_BUDGET.as_secs(),
            120
        );
    }

    #[test]
    fn read_back_declarations_reference_schema_attributes() {
        let configs = super::generated::configs();
        let mut declared = Vec::new();
        for config in configs {
            for requirement in config.await_on_read_back {
                assert!(
                    config
                        .schema
                        .attributes()
                        .contains_key(requirement.attribute()),
                    "{}.{} is absent",
                    config.resource_type_name,
                    requirement.attribute()
                );
                declared.push((
                    config.resource_type_name,
                    requirement.attribute().to_string(),
                ));
            }
        }
        assert_eq!(
            declared,
            [("acm.Certificate", "domain_validation_options".to_string())]
        );
    }

    #[test]
    fn configs_register_s3_bucket_under_both_kinds() {
        let configs = super::generated::configs();
        let managed = configs.iter().find(|c| {
            c.resource_type_name == "s3.Bucket" && c.schema.kind() == SchemaKind::Resource
        });
        let data_source = configs.iter().find(|c| {
            c.resource_type_name == "s3.Bucket" && c.schema.kind() == SchemaKind::DataSource
        });
        assert!(
            managed.is_some(),
            "Managed s3.Bucket missing from configs()"
        );
        assert!(
            data_source.is_some(),
            "DataSource s3.Bucket missing from configs()"
        );
    }

    #[test]
    fn only_organizations_account_prevents_destroy_by_default() {
        let protected = super::all_schemas()
            .into_iter()
            .filter(|schema| schema.as_schema().prevents_destroy_by_default())
            .map(|schema| schema.as_schema().resource_type().to_string())
            .collect::<Vec<_>>();

        assert_eq!(protected, vec!["organizations.Account"]);

        let bucket = super::all_schemas()
            .into_iter()
            .find(|schema| {
                let schema = schema.as_schema();
                schema.resource_type() == "s3.Bucket" && schema.kind() == SchemaKind::Resource
            })
            .expect("validated managed s3.Bucket schema");
        assert!(!bucket.as_schema().prevents_destroy_by_default());
    }

    fn collect_enum_identities(
        attr: &AttributeType,
        path: &str,
        identities: &mut Vec<(String, String)>,
    ) {
        match attr.shape_ref_free().expect("generated schema is Ref-free") {
            Shape::Enum { identity, .. } => {
                identities.push((path.to_string(), identity.to_string()))
            }
            Shape::List { element_type, .. } => {
                collect_enum_identities(element_type, &format!("{path}[]"), identities)
            }
            Shape::Map {
                key,
                value,
                constraints: _,
            } => {
                collect_enum_identities(key, &format!("{path}.<key>"), identities);
                collect_enum_identities(value, &format!("{path}.<value>"), identities);
            }
            Shape::Struct { .. } => {
                let mut budget = carina_core::schema::ShapeWalkBudget::new(64);
                let Some(fields) = attr
                    .struct_fields_ref_free_with_budget(&mut budget)
                    .expect("generated schema is Ref-free")
                else {
                    return;
                };
                for field in fields {
                    collect_enum_identities(
                        field.field_type(),
                        &format!("{path}.{}", field.name()),
                        identities,
                    );
                }
            }
            Shape::Union => {
                let mut budget = carina_core::schema::ShapeWalkBudget::new(64);
                let Some(members) = attr
                    .union_members_ref_free_with_budget(&mut budget)
                    .expect("generated schema is Ref-free")
                else {
                    return;
                };
                for (idx, member) in members.iter().enumerate() {
                    let member = member.expect("generated schema union member is Ref-free");
                    collect_enum_identities(
                        member.as_attr(),
                        &format!("{path}.<union:{idx}>"),
                        identities,
                    );
                }
            }
            _ => {}
        }
    }

    fn attr_type<'a, R: PatternForm>(
        schema: &'a PatternResourceSchema<R>,
        name: &str,
    ) -> &'a PatternAttributeType<R> {
        schema
            .attributes()
            .get(name)
            .unwrap_or_else(|| panic!("missing attribute {name}"))
            .attr_type()
    }

    fn assert_provider_populated_outputs(
        resource_type: &str,
        schema: &ResourceSchema,
        outputs: &[&str],
    ) {
        for output in outputs {
            let attribute = schema
                .attributes()
                .get(*output)
                .unwrap_or_else(|| panic!("missing {resource_type}.{output}"));
            assert!(
                attribute
                    .parts()
                    .declared_input_mode
                    .is_provider_populated(),
                "{resource_type}.{output} must be provider-populated"
            );
        }
    }

    #[test]
    fn data_source_outputs_are_provider_populated() {
        let iam_roles = super::generated::iam::roles::iam_roles_config();
        let identitystore_user = super::generated::identitystore::user::identitystore_user_config();
        let s3_bucket = super::generated::s3::bucket_data_source::s3_bucket_data_source_config();
        let caller_identity = super::generated::sts::caller_identity::sts_caller_identity_config();

        for (resource_type, schema, outputs) in [
            (
                "iam.Roles",
                &iam_roles.schema,
                &["arns", "names"] as &[&str],
            ),
            (
                "identitystore.User",
                &identitystore_user.schema,
                &["display_name", "emails"],
            ),
            (
                "s3.Bucket",
                &s3_bucket.schema,
                &[
                    "arn",
                    "region",
                    "bucket_domain_name",
                    "bucket_regional_domain_name",
                    "hosted_zone_id",
                ],
            ),
            (
                "sts.CallerIdentity",
                &caller_identity.schema,
                &["account_id", "arn", "user_id"],
            ),
        ] {
            assert_provider_populated_outputs(resource_type, schema, outputs);
        }
    }

    fn struct_field_type<'a, R: PatternForm>(
        schema: &'a PatternResourceSchema<R>,
        attr_name: &str,
        field_name: &str,
    ) -> &'a PatternAttributeType<R> {
        let mut budget = ShapeWalkBudget::new(8);
        schema
            .struct_fields_with_budget(attr_type(schema, attr_name), &mut budget)
            .unwrap_or_else(|| panic!("{attr_name} is not a struct"))
            .iter()
            .find(|field| field.name() == field_name)
            .unwrap_or_else(|| panic!("missing {attr_name}.{field_name}"))
            .field_type()
    }

    fn alias_target_value(hosted_zone_id: ConcreteValue) -> Value {
        Value::Concrete(ConcreteValue::Map(IndexMap::from([
            (
                "dns_name".to_string(),
                Value::Concrete(ConcreteValue::String(
                    "target.example.com".to_string().into(),
                )),
            ),
            (
                "evaluate_target_health".to_string(),
                Value::Concrete(ConcreteValue::Bool(false)),
            ),
            (
                "hosted_zone_id".to_string(),
                Value::Concrete(hosted_zone_id),
            ),
        ])))
    }

    fn aws_schema_registry() -> SchemaRegistry {
        super::schema_registry()
    }

    fn assert_refined_string<R: PatternForm>(
        attr: &PatternAttributeType<R>,
        expected_identity: &str,
        expected_pattern: Option<&str>,
    ) {
        let RawShape::String {
            identity, pattern, ..
        } = attr.raw_shape()
        else {
            panic!("expected refined String, got {:?}", attr.raw_shape());
        };
        assert_eq!(
            identity.map(|id| id.to_string()).as_deref(),
            Some(expected_identity)
        );
        assert_eq!(pattern, expected_pattern);
    }

    fn assert_refined_int_range(attr: &AttributeType, expected_range: (Option<i64>, Option<i64>)) {
        let RawShape::Int { range, .. } = attr.raw_shape() else {
            panic!("expected refined Int, got {:?}", attr.raw_shape());
        };
        assert_eq!(range, Some(expected_range));
    }

    #[test]
    fn acm_certificate_nested_string_enum_identities_only_repeat_the_acm_resource_record_type() {
        let config = super::generated::acm::certificate::acm_certificate_config();
        let mut identities = Vec::new();
        for (name, attr) in config.schema.attributes() {
            collect_enum_identities(attr.attr_type(), name, &mut identities);
        }

        let mut by_identity: HashMap<&str, Vec<&str>> = HashMap::new();
        for (path, identity) in &identities {
            by_identity
                .entry(identity.as_str())
                .or_default()
                .push(path.as_str());
        }
        let mut duplicates: Vec<_> = by_identity
            .into_iter()
            .filter(|(_, paths)| paths.len() > 1)
            .collect();
        for (_, paths) in &mut duplicates {
            paths.sort_unstable();
        }
        duplicates.sort_unstable_by_key(|(identity, _)| *identity);
        assert_eq!(
            duplicates,
            vec![(
                "aws.acm.Certificate.ResourceRecordType",
                vec![
                    "domain_validation_options[].resource_record.type",
                    "renewal_summary.domain_validation_options[].resource_record.type",
                ],
            )],
            "only the two ACM projections of Smithy's CNAME-only ResourceRecord.Type shape may share an enum identity"
        );

        let by_path: HashMap<_, _> = identities.into_iter().collect();
        assert_eq!(
            by_path.get("options.certificate_transparency_logging_preference"),
            Some(
                &"aws.acm.Certificate.CertificateOptions.CertificateTransparencyLoggingPreference"
                    .to_string()
            )
        );
        assert_eq!(
            by_path.get("options.export"),
            Some(&"aws.acm.Certificate.CertificateOptions.Export".to_string())
        );
        assert_eq!(
            by_path.get("options.validation_method"),
            Some(&"aws.acm.Certificate.CertificateOptions.ValidationMethod".to_string())
        );
        assert_eq!(
            by_path.get("domain_validation_options[].resource_record.type"),
            Some(&"aws.acm.Certificate.ResourceRecordType".to_string())
        );
        assert_eq!(
            by_path.get("domain_validation_options[].validation_method"),
            Some(&"aws.acm.Certificate.DomainValidation.ValidationMethod".to_string())
        );
        assert_eq!(
            by_path.get("renewal_summary.domain_validation_options[].resource_record.type"),
            Some(&"aws.acm.Certificate.ResourceRecordType".to_string())
        );
        assert_eq!(
            by_path.get("renewal_summary.domain_validation_options[].validation_method"),
            Some(
                &"aws.acm.Certificate.RenewalSummary.DomainValidation.ValidationMethod".to_string()
            )
        );
    }

    fn list_element_type<R: PatternForm>(
        attr: &PatternAttributeType<R>,
    ) -> &PatternAttributeType<R> {
        let Shape::List { element_type, .. } =
            attr.shape_ref_free().expect("generated schema is Ref-free")
        else {
            panic!("expected list, got {:?}", attr.raw_shape());
        };
        element_type
    }

    fn nested_struct_field_type<'a, R: PatternForm>(
        attr: &'a PatternAttributeType<R>,
        field_name: &str,
    ) -> &'a PatternAttributeType<R> {
        let mut budget = ShapeWalkBudget::new(16);
        attr.struct_fields_ref_free_with_budget(&mut budget)
            .expect("generated schema is Ref-free")
            .unwrap_or_else(|| panic!("expected struct while looking for {field_name}"))
            .iter()
            .find(|field| field.name() == field_name)
            .unwrap_or_else(|| panic!("missing nested field {field_name}"))
            .field_type()
    }

    fn enum_values(attr: &AttributeType) -> &[String] {
        let Shape::Enum {
            values: Some(values),
            ..
        } = attr.shape_ref_free().expect("generated schema is Ref-free")
        else {
            panic!("expected closed enum, got {:?}", attr.raw_shape());
        };
        values
    }

    #[test]
    fn acm_resource_record_type_is_distinct_from_route53_record_set_type() {
        let snapshot = loaded_aws();
        let registry = snapshot.schema_registry();
        let acm = registry
            .get("aws", "acm.Certificate", SchemaKind::Resource)
            .unwrap();
        let route53 = registry
            .get("aws", "route53.RecordSet", SchemaKind::Resource)
            .unwrap();

        let domain_validation =
            list_element_type(attr_type(acm.as_schema(), "domain_validation_options"));
        let resource_record = nested_struct_field_type(domain_validation, "resource_record");
        let acm_record_type = nested_struct_field_type(resource_record, "type");
        let route53_record_type = attr_type(route53.as_schema(), "type");

        let Shape::Enum {
            identity: acm_identity,
            ..
        } = acm_record_type
            .shape_ref_free()
            .expect("generated schema is Ref-free")
        else {
            panic!("ACM ResourceRecord.Type must be an enum");
        };
        let Shape::Enum {
            identity: route53_identity,
            ..
        } = route53_record_type
            .shape_ref_free()
            .expect("generated schema is Ref-free")
        else {
            panic!("Route 53 RecordSet.Type must be an enum");
        };

        assert_eq!(
            acm_identity.to_string(),
            "aws.acm.Certificate.ResourceRecordType"
        );
        assert_ne!(acm_identity, route53_identity);
        // The CNAME-only ACM enum and full Route 53 enum are nominally distinct.
        // Enum identities cannot be endpoints of provider assignability edges.
        assert!(
            !acm.as_schema()
                .type_in_schema(acm_record_type)
                .is_assignable_to(route53.assignment_sink(
                    route53_record_type,
                    &snapshot.provider_context().custom_types,
                )),
            "distinct ACM and Route 53 enum references must not assign implicitly"
        );
    }

    #[test]
    fn acm_resource_record_type_values_are_subset_of_route53_record_set_type_values() {
        let acm = super::generated::acm::certificate::acm_certificate_config();
        let route53 = super::generated::route53::record_set::route53_record_set_config();

        let domain_validation =
            list_element_type(attr_type(&acm.schema, "domain_validation_options"));
        let resource_record = nested_struct_field_type(domain_validation, "resource_record");
        let acm_values = enum_values(nested_struct_field_type(resource_record, "type"));
        let route53_values = enum_values(attr_type(&route53.schema, "type"));

        assert_eq!(acm_values, &["CNAME".to_string()]);
        assert!(
            acm_values
                .iter()
                .all(|value| route53_values.contains(value)),
            "ACM values {acm_values:?} must remain a subset of Route 53 values {route53_values:?}"
        );
    }

    #[test]
    fn route53_record_set_type_values_and_aliases_match_dns_record_type_catalog() {
        let route53 = super::generated::route53::record_set::route53_record_set_config();
        let route53_record_type = attr_type(&route53.schema, "type");
        let dns_record_type = AwsType::DnsRecordType.attribute_type();

        let Shape::Enum {
            values: Some(generated_values),
            dsl_aliases: generated_aliases,
            ..
        } = route53_record_type
            .shape_ref_free()
            .expect("generated schema is Ref-free")
        else {
            panic!("Route 53 RecordSet.Type must be a closed enum");
        };
        let RawShape::Enum {
            values: Some(catalog_values),
            dsl_aliases: catalog_aliases,
            ..
        } = dns_record_type.raw_shape()
        else {
            panic!("AwsType::DnsRecordType must be a closed enum");
        };

        assert_eq!(
            (generated_values, generated_aliases),
            (catalog_values, catalog_aliases),
            "Route 53 RecordSet.Type values and DSL aliases drifted from the shared catalog; update DNS_RECORD_TYPE_VALUES"
        );
    }

    #[test]
    fn generated_acceptance_refined_primitives_are_direct_shapes() {
        let iam_role = super::generated::iam::role::iam_role_config();
        assert_refined_int_range(
            attr_type(&iam_role.schema, "max_session_duration"),
            (Some(3600), Some(43200)),
        );
        assert_refined_string(
            attr_type(&iam_role.schema, "arn"),
            "aws.iam.Role.Arn",
            // #638: Pin all eight supported partitions in the generated IAM role ARN.
            Some(
                "^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):iam::[^:]*:role/.+$",
            ),
        );

        let route53_record_set = super::generated::route53::record_set::route53_record_set_config();
        assert_refined_int_range(
            attr_type(&route53_record_set.schema, "ttl"),
            (Some(0), Some(2147483647)),
        );

        let s3_bucket = super::generated::s3::bucket_data_source::s3_bucket_data_source_config();
        assert_refined_string(
            attr_type(&s3_bucket.schema, "arn"),
            "aws.s3.Bucket.Arn",
            // #638: Pin all eight supported partitions in the generated S3 bucket ARN.
            Some(
                "^arn:(?:aws|aws-cn|aws-us-gov|aws-iso|aws-iso-b|aws-iso-e|aws-iso-f|aws-eusc):s3:::.+$",
            ),
        );

        let ec2_vpc = super::generated::ec2::vpc::ec2_vpc_config();
        assert_refined_string(
            attr_type(&ec2_vpc.schema, "vpc_id"),
            "aws.ec2.Vpc.Id",
            Some("^vpc-[0-9a-f]{8,}$"),
        );

        let ec2_nat_gateway = super::generated::ec2::nat_gateway::ec2_nat_gateway_config();
        assert_refined_string(
            attr_type(&ec2_nat_gateway.schema, "subnet_id"),
            "aws.ec2.Subnet.Id",
            Some("^subnet-[0-9a-f]{8,}$"),
        );
        assert_refined_string(
            attr_type(&ec2_nat_gateway.schema, "vpc_id"),
            "aws.ec2.Vpc.Id",
            Some("^vpc-[0-9a-f]{8,}$"),
        );
    }

    #[test]
    fn generated_resource_unique_name_specs_follow_explicit_replacement_classes() {
        let acm_certificate = super::generated::acm::certificate::acm_certificate_config();
        // #649: a fresh ClientToken per create lets ACM certificates coexist.
        assert_eq!(
            acm_certificate.schema.unique_name(),
            &UniqueNameSpec::Coexisting
        );

        let iam_role = super::generated::iam::role::iam_role_config();
        assert_eq!(
            iam_role.schema.unique_name(),
            &UniqueNameSpec::Attribute("role_name".to_string())
        );

        let ec2_vpc = super::generated::ec2::vpc::ec2_vpc_config();
        assert_eq!(ec2_vpc.schema.unique_name(), &UniqueNameSpec::Conflicting);

        let ec2_route = super::generated::ec2::route::ec2_route_config();
        assert_eq!(ec2_route.schema.unique_name(), &UniqueNameSpec::Coexisting);

        let ec2_security_group = super::generated::ec2::security_group::ec2_security_group_config();
        assert_eq!(
            ec2_security_group.schema.unique_name(),
            &UniqueNameSpec::Attribute("group_name".to_string())
        );

        let ec2_vpc_gateway_attachment =
            super::generated::ec2::vpc_gateway_attachment::ec2_vpc_gateway_attachment_config();
        assert_eq!(
            ec2_vpc_gateway_attachment.schema.unique_name(),
            &UniqueNameSpec::Conflicting
        );

        let ec2_vpc_peering_connection =
            super::generated::ec2::vpc_peering_connection::ec2_vpc_peering_connection_config();
        assert_eq!(
            ec2_vpc_peering_connection.schema.unique_name(),
            &UniqueNameSpec::Conflicting
        );

        let route53_record_set = super::generated::route53::record_set::route53_record_set_config();
        assert_eq!(
            route53_record_set.schema.unique_name(),
            &UniqueNameSpec::Conflicting
        );

        let s3_bucket_policy = super::generated::s3::bucket_policy::s3_bucket_policy_config();
        assert_eq!(
            s3_bucket_policy.schema.unique_name(),
            &UniqueNameSpec::Coexisting
        );

        let s3_bucket = super::generated::s3::bucket::s3_bucket_config();
        assert_eq!(s3_bucket.schema.unique_name(), &UniqueNameSpec::Conflicting);

        let sqs_queue = super::generated::sqs::queue::sqs_queue_config();
        assert_eq!(sqs_queue.schema.unique_name(), &UniqueNameSpec::Conflicting);
    }

    #[test]
    fn declared_identity_uses_only_the_resource_key_attributes() {
        let configs = super::generated::configs();
        for (resource_type, expected) in [
            ("route53.RecordSet", vec!["hosted_zone_id", "name", "type"]),
            ("iam.Role", vec!["role_name"]),
            ("logs.LogGroup", vec!["log_group_name"]),
            ("sqs.Queue", vec!["queue_name"]),
            (
                "ec2.TransitGatewayAttachment",
                vec!["transit_gateway_id", "vpc_id"],
            ),
        ] {
            let config = configs
                .iter()
                .find(|config| config.resource_type_name == resource_type)
                .unwrap_or_else(|| panic!("missing {resource_type} schema"));
            let identity: Vec<_> = config
                .schema
                .attributes()
                .values()
                .filter(|attr| attr.identity_mode().is_identity())
                .map(|attr| {
                    assert!(attr.is_create_only(), "{resource_type}.{}", attr.name());
                    assert_eq!(attr.identity_mode(), IdentityMode::Required);
                    attr.name()
                })
                .collect();
            assert_eq!(identity, expected, "{resource_type}");
            config
                .schema
                .clone()
                .validate()
                .expect("declared identity must pass admission");
        }
    }

    #[test]
    fn route53_alias_target_hosted_zone_id_enforces_assignable_source_identities() {
        let snapshot = loaded_aws();
        let record_set = snapshot
            .schema_registry()
            .get("aws", "route53.RecordSet", SchemaKind::Resource)
            .unwrap();
        let sink = struct_field_type(record_set.as_schema(), "alias_target", "hosted_zone_id");
        assert!(matches!(
            record_set.as_schema().shape_of(sink),
            Shape::Union
        ));
        let sink = record_set.assignment_sink(sink, &snapshot.provider_context().custom_types);
        let sources = ValidatedConfigSchema::validate(std::collections::BTreeMap::from([
            (
                "route53_id".into(),
                carina_aws_types::route53_hosted_zone_id(),
            ),
            (
                "cloudfront_id".into(),
                carina_aws_types::cloudfront_hosted_zone_id(),
            ),
            ("plain_string".into(), AttributeType::string()),
            ("unrelated_id".into(), carina_aws_types::iam_role_arn()),
        ]))
        .expect("assignment source types must pass admission");

        let route53_id = &sources.parts()["route53_id"];
        assert!(
            TypeInSchema::schemaless(route53_id).is_assignable_to(sink),
            "Route 53 hosted-zone IDs must be assignable to alias_target.hosted_zone_id"
        );

        let cloudfront_id = &sources.parts()["cloudfront_id"];
        assert!(
            TypeInSchema::schemaless(cloudfront_id).is_assignable_to(sink),
            "the CloudFront global hosted-zone enum must remain assignable"
        );

        let plain_string = &sources.parts()["plain_string"];
        assert!(
            !TypeInSchema::schemaless(plain_string).is_assignable_to(sink),
            "an unrefined String source must not satisfy an identified sink"
        );

        let unrelated_id = &sources.parts()["unrelated_id"];
        assert!(
            !TypeInSchema::schemaless(unrelated_id).is_assignable_to(sink),
            "an unrelated identified String must not satisfy the hosted-zone sink"
        );
    }

    #[test]
    fn route53_alias_target_hosted_zone_id_validates_cloudfront_and_route53_values() {
        let record_set = super::generated::route53::record_set::route53_record_set_config();
        let schema = record_set
            .schema
            .validate()
            .expect("Route 53 RecordSet schema must pass admission");
        let validate_attributes = |attributes: &HashMap<String, Value>| {
            attributes.iter().try_for_each(|(name, value)| {
                schema
                    .effective_attribute(name)
                    .unwrap_or_else(|| panic!("missing RecordSet attribute {name}"))
                    .validate(value)
            })
        };
        let accepted = [
            (
                "CloudFront namespaced constant",
                ConcreteValue::enum_identifier("aws.cloudfront.HostedZoneId.global"),
            ),
            (
                "CloudFront wire literal",
                ConcreteValue::String("Z2FDTNDATAQYW2".to_string().into()),
            ),
            (
                "ALB hosted-zone literal",
                ConcreteValue::String("Z14GRHDCWA56QT".to_string().into()),
            ),
            (
                "measured Route 53 hosted-zone literal",
                ConcreteValue::String("Z05136711ZXUHBDOA8D5O".to_string().into()),
            ),
        ];

        for (label, hosted_zone_id) in accepted {
            let attributes = HashMap::from([
                (
                    "alias_target".to_string(),
                    alias_target_value(hosted_zone_id),
                ),
                (
                    "name".to_string(),
                    Value::Concrete(ConcreteValue::String("www.example.com".to_string().into())),
                ),
                (
                    "type".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier(
                        "aws.route53.RecordSet.Type.a",
                    )),
                ),
            ]);
            let result = validate_attributes(&attributes);
            assert!(result.is_ok(), "{label} should validate: {result:?}");
        }

        let too_long = format!("Z{}", "A".repeat(32));
        assert_eq!(too_long.len(), 33);
        for (label, hosted_zone_id) in [
            ("empty literal", ""),
            ("bare enum member as a quoted string", "global"),
            (
                "qualified DSL path as a quoted string",
                "aws.cloudfront.HostedZoneId.global",
            ),
            ("lowercase hosted-zone literal", "z14grhdcwa56qt"),
            (
                "Route 53 API path-prefixed ID",
                "/hostedzone/Z05136711ZXUHBDOA8D5O",
            ),
            ("33-character hosted-zone literal", too_long.as_str()),
        ] {
            let attributes = HashMap::from([
                (
                    "alias_target".to_string(),
                    alias_target_value(ConcreteValue::String(hosted_zone_id.to_string().into())),
                ),
                (
                    "name".to_string(),
                    Value::Concrete(ConcreteValue::String("www.example.com".to_string().into())),
                ),
                (
                    "type".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier(
                        "aws.route53.RecordSet.Type.a",
                    )),
                ),
            ]);
            let result = validate_attributes(&attributes);
            assert!(result.is_err(), "{label} should be rejected");
        }
    }

    async fn normalize_alias_target_hosted_zone_id_and_assert_no_change(
        label: &str,
        desired_hosted_zone_id: ConcreteValue,
        read_back_hosted_zone_id: &str,
    ) -> ConcreteValue {
        let registry = aws_schema_registry();
        let mut desired = Resource::with_provider("aws", "route53.RecordSet", "record", None);
        desired.set_attr("alias_target", alias_target_value(desired_hosted_zone_id));
        let id = desired.id.clone();

        let normalized = apply_desired_normalization(desired, &[], &AwsNormalizer, &registry)
            .await
            .expect("desired normalization should succeed");
        let Some(Value::Concrete(ConcreteValue::Map(alias_target))) =
            normalized.as_resource().get_attr("alias_target")
        else {
            panic!("normalized alias_target must remain a map");
        };
        let Some(Value::Concrete(normalized_hosted_zone_id)) = alias_target.get("hosted_zone_id")
        else {
            panic!("normalized hosted_zone_id must be concrete");
        };
        let normalized_hosted_zone_id = normalized_hosted_zone_id.clone();

        let current_attributes = HashMap::from([(
            "alias_target".to_string(),
            alias_target_value(ConcreteValue::String(
                read_back_hosted_zone_id.to_string().into(),
            )),
        )]);
        let mut current_states = HashMap::from([(
            id.clone(),
            crate::read_model::test_existing_state(id.clone(), current_attributes).to_snapshot(),
        )]);
        normalize_provider_states(&AwsNormalizer, &mut current_states)
            .await
            .expect("state normalization should succeed");
        let mut current_states = current_states
            .into_iter()
            .map(|(id, state)| (id, Current::from_snapshot(state)))
            .collect();
        carina_core::value::canonicalize_states_with_schemas(&mut current_states, &registry);
        let current = match current_states.remove(&id).expect("current RecordSet state") {
            Current::Absent(state) => state.into_state(),
            Current::Present(present) => present.into_parts().0,
            Current::CreateInFlight(_pending) => {
                panic!("test fixture must not become an in-flight create")
            }
            Current::CreateOutcomeUnknown(_unknown) => {
                panic!("test fixture must not become an unknown create outcome")
            }
            Current::MissingIdentifier(state) => state.into_state(),
        };
        let router = crate::test_comparison_router();
        let comparison = router
            .comparison_for(&registry, &id)
            .expect("RecordSet comparison context");
        let saved_attrs = carina_core::provider::RawSavedAttrs::default().lift(&registry);
        let desired = ResolvedResource::new(
            normalized
                .as_resource()
                .clone()
                .into_resolved_dependency_phase_for_test(),
        );
        let result = crate::test_diff(&desired, &current, &saved_attrs, None, comparison);
        assert!(
            matches!(result, Diff::NoChange(_)),
            "{label}: read-back wire value must not cause a phantom diff: {result:?}"
        );

        normalized_hosted_zone_id
    }

    #[tokio::test]
    async fn route53_alias_target_cloudfront_global_normalizes_and_diffs_without_change() {
        let hosted_zone_id = normalize_alias_target_hosted_zone_id_and_assert_no_change(
            "CloudFront namespaced constant",
            ConcreteValue::enum_identifier("aws.cloudfront.HostedZoneId.global"),
            "Z2FDTNDATAQYW2",
        )
        .await;
        let ConcreteValue::CanonicalEnum(canonical) = hosted_zone_id else {
            panic!("CloudFront global must canonicalize to CanonicalEnum");
        };
        assert_eq!(canonical.api_value(), "Z2FDTNDATAQYW2");
    }

    #[tokio::test]
    async fn route53_alias_target_literal_hosted_zone_ids_normalize_and_diff_without_change() {
        for (label, hosted_zone_id) in [
            ("ALB hosted-zone literal", "Z14GRHDCWA56QT"),
            ("CloudFront wire literal", "Z2FDTNDATAQYW2"),
        ] {
            let normalized = normalize_alias_target_hosted_zone_id_and_assert_no_change(
                label,
                ConcreteValue::String(hosted_zone_id.to_string().into()),
                hosted_zone_id,
            )
            .await;
            match normalized {
                ConcreteValue::CanonicalEnum(canonical) => {
                    assert_eq!(canonical.api_value(), hosted_zone_id, "{label}")
                }
                ConcreteValue::String(value) => assert_eq!(value, hosted_zone_id, "{label}"),
                other => panic!("{label}: unexpected normalized value: {other:?}"),
            }
        }
    }

    #[test]
    fn s3_bucket_hosted_zone_id_outputs_are_assignable_to_route53_alias_target() {
        let snapshot = loaded_aws();
        let registry = snapshot.schema_registry();
        let record_set = registry
            .get("aws", "route53.RecordSet", SchemaKind::Resource)
            .unwrap();
        let sink = struct_field_type(record_set.as_schema(), "alias_target", "hosted_zone_id");

        for (label, kind) in [
            ("s3.Bucket resource", SchemaKind::Resource),
            ("s3.Bucket data source", SchemaKind::DataSource),
        ] {
            let bucket = registry.get("aws", "s3.Bucket", kind).unwrap();
            let source_schema = bucket.as_schema();
            let source = attr_type(source_schema, "hosted_zone_id");
            assert_refined_string(source, "aws.route53.HostedZone.Id", Some("^Z[A-Z0-9]+$"));
            assert!(
                source_schema.type_in_schema(source).is_assignable_to(
                    record_set.assignment_sink(sink, &snapshot.provider_context().custom_types,)
                ),
                "{label} hosted_zone_id must be assignable to the Route 53 alias target"
            );
        }
    }

    #[test]
    fn security_group_ip_protocol_values_are_canonical_and_keep_dsl_alias() {
        for resource_type in ["ec2.SecurityGroupIngress", "ec2.SecurityGroupEgress"] {
            let config = super::generated::configs()
                .into_iter()
                .find(|config| config.resource_type_name == resource_type)
                .unwrap_or_else(|| panic!("{resource_type} config should exist"));
            let attr_type = config
                .schema
                .attributes()
                .get("ip_protocol")
                .unwrap_or_else(|| panic!("{resource_type}.ip_protocol should exist"))
                .attr_type();
            let Shape::Enum {
                values: Some(values),
                dsl_aliases,
                ..
            } = config.schema.shape_of(attr_type)
            else {
                panic!("{resource_type}.ip_protocol should be an enum");
            };

            assert!(
                values.iter().any(|value| value == "tcp")
                    && values.iter().any(|value| value == "udp")
                    && values.iter().any(|value| value == "icmp")
                    && values.iter().any(|value| value == "icmpv6")
                    && values.iter().any(|value| value == "-1"),
                "{resource_type}.ip_protocol should include canonical wire values: {values:?}"
            );
            assert!(
                !values.iter().any(|value| value == "all")
                    && !values.iter().any(|value| value == "_1"),
                "{resource_type}.ip_protocol values must not include DSL aliases: {values:?}"
            );
            assert!(
                dsl_aliases
                    .iter()
                    .any(|(api, dsl)| api == "-1" && dsl == "all"),
                "{resource_type}.ip_protocol must retain the all -> -1 schema alias: {dsl_aliases:?}"
            );
        }
    }
}
