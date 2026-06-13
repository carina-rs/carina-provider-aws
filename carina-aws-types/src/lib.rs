//! Shared AWS type definitions and validators
//!
//! This module contains type validators shared between `carina-provider-aws`
//! and `carina-provider-awscc`. Provider-specific types (region with namespace,
//! schema config structs) remain in their respective crates.

mod common;
mod services;

pub use common::*;
pub use services::*;

#[cfg(test)]
mod tests {
    use carina_core::resource::{ConcreteValue, Value};
    use carina_core::schema::{AttributeType, DslTransform};

    use crate::*;

    // Custom type shape tests

    fn field_type<'a>(attr: &'a AttributeType, field_name: &str) -> &'a AttributeType {
        let mut budget = carina_core::schema::ShapeWalkBudget::new(8);
        let Some(fields) = attr
            .struct_fields_ref_free_with_budget(&mut budget)
            .expect("test schema is Ref-free")
        else {
            panic!("expected Struct shape");
        };
        &fields
            .iter()
            .find(|field| field.name == field_name)
            .unwrap_or_else(|| panic!("missing field {field_name}"))
            .field_type
    }

    fn list_inner(attr: &AttributeType) -> &AttributeType {
        let carina_core::schema::Shape::List { element_type, .. } =
            attr.shape_ref_free().expect("test schema is Ref-free")
        else {
            panic!("expected List shape");
        };
        element_type
    }

    fn struct_name(attr: &AttributeType) -> &str {
        let carina_core::schema::Shape::Struct { name, .. } =
            attr.shape_ref_free().expect("test schema is Ref-free")
        else {
            panic!("expected Struct shape");
        };
        name
    }

    fn union_member(attr: &AttributeType, index: usize) -> &AttributeType {
        let mut budget = carina_core::schema::ShapeWalkBudget::new(8);
        let Some(members) = attr
            .union_members_ref_free_with_budget(&mut budget)
            .expect("test schema is Ref-free")
        else {
            panic!("expected Union shape");
        };
        &members[index]
    }

    fn string_enum_identity(attr: &AttributeType) -> String {
        let carina_core::schema::Shape::Enum { identity, .. } =
            attr.shape_ref_free().expect("test schema is Ref-free")
        else {
            panic!("expected enum shape with identity");
        };
        identity.to_string()
    }

    fn assert_refined_string_identity(attr: &AttributeType, expected: &str) {
        if let carina_core::schema::Shape::String { identity, .. } =
            attr.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(identity.map(|id| id.to_string()).as_deref(), Some(expected));
        } else {
            panic!("expected refined String");
        }
    }

    fn assert_string_attr_accepts_and_rejects(attr: AttributeType, accepted: &str, rejected: &str) {
        let schema = carina_core::schema::Schema::flat(attr);
        assert!(
            schema
                .validate(&Value::Concrete(ConcreteValue::String(
                    accepted.to_string()
                )))
                .is_ok(),
            "expected '{accepted}' to validate"
        );
        assert!(
            schema
                .validate(&Value::Concrete(ConcreteValue::String(
                    rejected.to_string()
                )))
                .is_err(),
            "expected '{rejected}' to be rejected"
        );
    }

    #[test]
    fn iam_policy_document_struct_names_are_plain_and_effect_identity_is_structural() {
        let policy_document = iam_policy_document();
        let policy_statement = list_inner(field_type(&policy_document, "statement"));
        let principal = union_member(field_type(policy_statement, "principal"), 0);

        assert_eq!(struct_name(&policy_document), "PolicyDocument");
        assert_eq!(struct_name(policy_statement), "Statement");
        assert_eq!(struct_name(principal), "Principal");
        assert_eq!(
            string_enum_identity(&iam_policy_effect()),
            "aws.iam.PolicyDocument.Statement.Effect"
        );
    }

    #[test]
    fn sqs_redrive_policy_struct_names_are_plain() {
        assert_eq!(struct_name(&sqs_redrive_policy()), "RedrivePolicy");
        assert_eq!(
            struct_name(&sqs_redrive_allow_policy()),
            "RedriveAllowPolicy"
        );
    }

    #[test]
    fn hand_written_string_enum_identities_are_structural() {
        let policy_document = iam_policy_document();
        let policy_statement = list_inner(field_type(&policy_document, "statement"));

        let queue_redrive_allow_policy = sqs_redrive_allow_policy();

        let sse_rules = bucket_encryption_rules();
        let sse_rule = list_inner(&sse_rules);
        let sse_by_default = field_type(sse_rule, "apply_server_side_encryption_by_default");

        let replication_rules = bucket_replication_rules();
        let replication_rule = list_inner(&replication_rules);
        let replication_destination = field_type(replication_rule, "destination");
        let delete_marker_replication = field_type(replication_rule, "delete_marker_replication");

        let lifecycle_rules = bucket_lifecycle_rules();
        let lifecycle_rule = list_inner(&lifecycle_rules);
        let lifecycle_transition = list_inner(field_type(lifecycle_rule, "transitions"));

        let target_object_key_format = bucket_target_object_key_format();
        let partitioned_prefix = field_type(&target_object_key_format, "partitioned_prefix");

        let redirect_all_requests_to = s3_redirect_all_requests_to();

        let actual = vec![
            (
                "iam_policy_effect",
                string_enum_identity(field_type(policy_statement, "effect")),
            ),
            (
                "iam_policy_version",
                string_enum_identity(field_type(&policy_document, "version")),
            ),
            (
                "sqs_redrive_permission",
                string_enum_identity(field_type(
                    &queue_redrive_allow_policy,
                    "redrive_permission",
                )),
            ),
            (
                "s3_sse_algorithm",
                string_enum_identity(field_type(sse_by_default, "sse_algorithm")),
            ),
            (
                "s3_replication_destination.storage_class",
                string_enum_identity(field_type(replication_destination, "storage_class")),
            ),
            (
                "s3_replication_status",
                string_enum_identity(field_type(replication_rule, "status")),
            ),
            (
                "s3_delete_marker_replication.status",
                string_enum_identity(field_type(delete_marker_replication, "status")),
            ),
            (
                "s3_lifecycle_status",
                string_enum_identity(field_type(lifecycle_rule, "status")),
            ),
            (
                "s3_transition_storage_class",
                string_enum_identity(field_type(lifecycle_transition, "storage_class")),
            ),
            (
                "s3_partition_date_source",
                string_enum_identity(field_type(partitioned_prefix, "partition_date_source")),
            ),
            (
                "s3_redirect_all_requests_to.protocol",
                string_enum_identity(field_type(&redirect_all_requests_to, "protocol")),
            ),
        ];

        let expected = vec![
            (
                "iam_policy_effect",
                "aws.iam.PolicyDocument.Statement.Effect".to_string(),
            ),
            (
                "iam_policy_version",
                "aws.iam.PolicyDocument.Version".to_string(),
            ),
            (
                "sqs_redrive_permission",
                "aws.sqs.Queue.RedriveAllowPolicy.RedrivePermission".to_string(),
            ),
            (
                "s3_sse_algorithm",
                "aws.s3.BucketServerSideEncryptionConfiguration.SseRule.SseByDefault.SseAlgorithm"
                    .to_string(),
            ),
            (
                "s3_replication_destination.storage_class",
                "aws.s3.BucketReplicationConfiguration.ReplicationRule.ReplicationDestination.StorageClass"
                    .to_string(),
            ),
            (
                "s3_replication_status",
                "aws.s3.BucketReplicationConfiguration.ReplicationRule.Status".to_string(),
            ),
            (
                "s3_delete_marker_replication.status",
                "aws.s3.BucketReplicationConfiguration.ReplicationRule.DeleteMarkerReplication.Status"
                    .to_string(),
            ),
            (
                "s3_lifecycle_status",
                "aws.s3.BucketLifecycleConfiguration.LifecycleRule.Status".to_string(),
            ),
            (
                "s3_transition_storage_class",
                "aws.s3.BucketLifecycleConfiguration.LifecycleRule.LifecycleTransition.StorageClass"
                    .to_string(),
            ),
            (
                "s3_partition_date_source",
                "aws.s3.BucketLogging.TargetObjectKeyFormat.PartitionedPrefix.PartitionDateSource"
                    .to_string(),
            ),
            (
                "s3_redirect_all_requests_to.protocol",
                "aws.s3.BucketWebsiteConfiguration.RedirectAllRequestsTo.Protocol".to_string(),
            ),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn aws_account_id_carries_pattern_and_length() {
        let t = aws_account_id();
        if let carina_core::schema::Shape::String {
            identity,
            pattern,
            length,
            to_dsl: None,
            ..
        } = t.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(
                identity.map(|id| id.to_string()).as_deref(),
                Some("aws.AccountId")
            );
            assert_eq!(pattern, Some(r"^\d{12}$"));
            assert_eq!(length, Some((Some(12), Some(12))));
        } else {
            panic!("aws_account_id() should be refined String");
        }
    }

    #[test]
    fn vpc_id_carries_identity_and_pattern() {
        let t = vpc_id();
        if let carina_core::schema::Shape::String {
            identity,
            pattern,
            to_dsl: None,
            ..
        } = t.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(
                identity.map(|id| id.to_string()).as_deref(),
                Some("aws.ec2.Vpc.Id")
            );
            assert!(pattern.is_some(), "VpcId should carry a pattern");
        } else {
            panic!("vpc_id() should be refined String");
        }
    }

    // ARN tests

    #[test]
    fn validate_arn_valid() {
        assert!(validate_arn("arn:aws:s3:::my-bucket").is_ok());
        assert!(validate_arn("arn:aws:iam::123456789012:role/MyRole").is_ok());
        assert!(validate_arn("arn:aws-cn:s3:::my-bucket").is_ok());
        assert!(validate_arn("arn:aws:ec2:us-east-1:123456789012:vpc/vpc-1234").is_ok());
    }

    #[test]
    fn validate_arn_invalid() {
        assert!(validate_arn("not-an-arn").is_err());
        assert!(validate_arn("arn:aws:s3").is_err());
        assert!(validate_arn("arn:aws").is_err());
        assert!(validate_arn("").is_err());
    }

    #[test]
    fn validate_arn_rejects_empty_partition() {
        // "arn::::::" has empty partition and service — should be rejected
        assert!(validate_arn("arn::s3:::my-bucket").is_err());
        assert!(validate_arn("arn:::::").is_err());
    }

    #[test]
    fn validate_arn_rejects_empty_service() {
        assert!(validate_arn("arn:aws::::").is_err());
        assert!(validate_arn("arn:aws:::123456789012:resource").is_err());
    }

    #[test]
    fn validate_arn_type_with_value() {
        let t = arn();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "arn:aws:s3:::my-bucket".to_string()
                )))
                .is_ok()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "not-an-arn".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::Int(42)))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::resource_ref(
                    "role".to_string(),
                    "arn".to_string(),
                    vec![]
                ))
                .is_ok()
        );
    }

    // Resource ID tests

    #[test]
    fn validate_aws_resource_id_valid() {
        assert!(validate_aws_resource_id("vpc-1a2b3c4d").is_ok());
        assert!(validate_aws_resource_id("subnet-0123456789abcdef0").is_ok());
        assert!(validate_aws_resource_id("sg-12345678").is_ok());
        assert!(validate_aws_resource_id("rtb-abcdef12").is_ok());
        assert!(validate_aws_resource_id("eipalloc-0123456789abcdef0").is_ok());
        assert!(validate_aws_resource_id("igw-12345678").is_ok());
    }

    #[test]
    fn validate_aws_resource_id_invalid() {
        assert!(validate_aws_resource_id("not-a-valid-id").is_err());
        assert!(validate_aws_resource_id("vpc").is_err());
        assert!(validate_aws_resource_id("vpc-short").is_err());
        assert!(validate_aws_resource_id("vpc-1234567").is_err());
        assert!(validate_aws_resource_id("VPC-12345678").is_err());
    }

    #[test]
    fn validate_aws_resource_id_type_with_value() {
        let t = aws_resource_id();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "vpc-1a2b3c4d".to_string()
                )))
                .is_ok()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String("vpc".to_string())))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::Int(42)))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::resource_ref(
                    "my_vpc".to_string(),
                    "vpc_id".to_string(),
                    vec![]
                ))
                .is_ok()
        );
    }

    #[test]
    fn validate_vpc_cidr_block_association_id_valid() {
        let t = vpc_cidr_block_association_id();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "vpc-cidr-assoc-12345678".to_string()
                )))
                .is_ok()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "vpc-cidr-assoc-0123456789abcdef0".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn validate_vpc_cidr_block_association_id_invalid() {
        let t = vpc_cidr_block_association_id();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "vpc-12345678".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "invalid".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn validate_tgw_route_table_id_valid() {
        let t = tgw_route_table_id();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "tgw-rtb-12345678".to_string()
                )))
                .is_ok()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "tgw-rtb-0123456789abcdef0".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn validate_tgw_route_table_id_invalid() {
        let t = tgw_route_table_id();
        // Regular route table ID should fail
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "rtb-12345678".to_string()
                )))
                .is_err()
        );
        // Transit gateway ID should fail
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "tgw-12345678".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "invalid".to_string()
                )))
                .is_err()
        );
    }

    // Availability zone tests

    #[test]
    fn validate_availability_zone_valid() {
        assert!(validate_availability_zone("us-east-1a").is_ok());
        assert!(validate_availability_zone("ap-northeast-1c").is_ok());
        assert!(validate_availability_zone("eu-central-1b").is_ok());
        assert!(validate_availability_zone("me-south-1a").is_ok());
        assert!(validate_availability_zone("us-west-2d").is_ok());
    }

    #[test]
    fn validate_availability_zone_local_zone() {
        // Local Zones: us-east-1-bos-1a, us-west-2-lax-1a
        assert!(validate_availability_zone("us-east-1-bos-1a").is_ok());
        assert!(validate_availability_zone("us-west-2-lax-1a").is_ok());
        assert!(validate_availability_zone("ap-northeast-1-tpe-1a").is_ok());
    }

    #[test]
    fn validate_availability_zone_wavelength_zone() {
        // Wavelength Zones: us-east-1-wl1-bos-wlz-1
        assert!(validate_availability_zone("us-east-1-wl1-bos-wlz-1").is_ok());
        assert!(validate_availability_zone("us-west-2-wl1-las-wlz-1").is_ok());
    }

    #[test]
    fn validate_availability_zone_invalid() {
        assert!(validate_availability_zone("us-east-1").is_err()); // region, not AZ
        assert!(validate_availability_zone("a").is_err()); // too short
        assert!(validate_availability_zone("invalid").is_err()); // no numeric part
        assert!(validate_availability_zone("us-east").is_err()); // no numeric part
    }

    #[test]
    fn az_accepts_aws_format() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "us-east-1a".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn az_accepts_dsl_format() {
        let az_type = availability_zone();
        // The full DSL form for the zone-name variant carries the
        // `ZoneName` kind segment — `aws.AvailabilityZone.ZoneName.<v>` —
        // matching the structured identity's dotted display.
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "aws.AvailabilityZone.ZoneName.us_east_1a".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn az_accepts_shorthand_format() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "us_east_1a".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn az_accepts_kind_shorthand_format() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "ZoneName.us_east_1a".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn az_rejects_invalid_az() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "invalid-zone".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn az_rejects_wrong_namespace() {
        let az_type = availability_zone();
        assert!(
            carina_core::schema::Schema::flat(az_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "gcp.AvailabilityZone.ZoneName.us_east_1a".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn az_has_namespace() {
        // Post-#3222: AZ is an enum, not a `Custom`. The legacy
        // `namespace: Some("aws")` field is now derived from the
        // structured identity via `dotted_prefix()`.
        let az_type = availability_zone();
        if let carina_core::schema::Shape::Enum { identity, .. } =
            az_type.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(
                identity.dotted_prefix().as_deref(),
                Some("aws.AvailabilityZone")
            );
        } else {
            panic!("Expected enum type");
        }
    }

    #[test]
    fn az_has_to_dsl() {
        let az_type = availability_zone();
        if let carina_core::schema::Shape::Enum { to_dsl, .. } =
            az_type.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(to_dsl, Some(&DslTransform::HyphenToUnderscore));
            assert_eq!(
                DslTransform::HyphenToUnderscore.apply("us-east-1a"),
                "us_east_1a"
            );
        } else {
            panic!("Expected enum type");
        }
    }

    // Availability zone ID tests

    #[test]
    fn validate_availability_zone_id_valid() {
        assert!(validate_availability_zone_id("use1-az1").is_ok());
        assert!(validate_availability_zone_id("use1-az2").is_ok());
        assert!(validate_availability_zone_id("usw2-az1").is_ok());
        assert!(validate_availability_zone_id("usw2-az4").is_ok());
        assert!(validate_availability_zone_id("apne1-az1").is_ok());
        assert!(validate_availability_zone_id("apne1-az4").is_ok());
        assert!(validate_availability_zone_id("euc1-az1").is_ok());
        assert!(validate_availability_zone_id("aps1-az1").is_ok());
        assert!(validate_availability_zone_id("mes1-az1").is_ok());
        assert!(validate_availability_zone_id("afs1-az1").is_ok());
    }

    #[test]
    fn validate_availability_zone_id_invalid() {
        assert!(validate_availability_zone_id("us-east-1a").is_err()); // AZ name, not ID
        assert!(validate_availability_zone_id("use1").is_err()); // missing -az suffix
        assert!(validate_availability_zone_id("az1").is_err()); // missing region prefix
        assert!(validate_availability_zone_id("").is_err()); // empty
        assert!(validate_availability_zone_id("USE1-AZ1").is_err()); // uppercase
        assert!(validate_availability_zone_id("use-az1").is_err()); // prefix doesn't end with digit
        assert!(validate_availability_zone_id("use1-az").is_err()); // missing AZ number
        assert!(validate_availability_zone_id("use1-azX").is_err()); // non-digit after -az
    }

    #[test]
    fn validate_availability_zone_id_type_with_value() {
        let t = availability_zone_id();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "use1-az1".to_string()
                )))
                .is_ok()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "us-east-1a".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::Int(42)))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::resource_ref(
                    "subnet".to_string(),
                    "availability_zone_id".to_string(),
                    vec![]
                ))
                .is_ok()
        );
    }

    // Enum helpers

    #[test]
    fn find_matching_enum_value_exact() {
        let values = &["Enabled", "Suspended"];
        assert_eq!(find_matching_enum_value("Enabled", values), Some("Enabled"));
        assert_eq!(find_matching_enum_value("Missing", values), None);
    }

    #[test]
    fn find_matching_enum_value_case_insensitive() {
        let values = &["Enabled", "Suspended"];
        assert_eq!(find_matching_enum_value("enabled", values), Some("Enabled"));
    }

    #[test]
    fn find_matching_enum_value_underscore_to_hyphen() {
        let values = &["us-east-1", "eu-west-1"];
        assert_eq!(
            find_matching_enum_value("us_east_1", values),
            Some("us-east-1")
        );
    }

    #[test]
    fn canonicalize_enum_value_matches() {
        assert_eq!(
            canonicalize_enum_value("enabled", &["Enabled", "Suspended"]),
            "Enabled"
        );
    }

    #[test]
    fn canonicalize_enum_value_no_match() {
        assert_eq!(
            canonicalize_enum_value("unknown", &["Enabled", "Suspended"]),
            "unknown"
        );
    }

    // IPAM Pool ID tests

    #[test]
    fn validate_ipam_pool_id_valid() {
        assert!(validate_ipam_pool_id("ipam-pool-0123456789abcdef0").is_ok());
        assert!(validate_ipam_pool_id("ipam-pool-12345678").is_ok());
    }

    #[test]
    fn validate_ipam_pool_id_invalid() {
        assert!(validate_ipam_pool_id("ipam-pool-short").is_err());
        assert!(validate_ipam_pool_id("not-ipam-pool").is_err());
        assert!(validate_ipam_pool_id("ipam-pool-").is_err());
    }

    // AWS Account ID tests

    #[test]
    fn validate_aws_account_id_valid() {
        assert!(validate_aws_account_id("123456789012").is_ok());
    }

    #[test]
    fn validate_aws_account_id_invalid() {
        assert!(validate_aws_account_id("1234").is_err());
        assert!(validate_aws_account_id("12345678901a").is_err());
        assert!(validate_aws_account_id("").is_err());
    }

    // KMS Key ID tests

    #[test]
    fn validate_kms_key_id_arn() {
        assert!(
            validate_kms_key_id(
                "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab"
            )
            .is_ok()
        );
        assert!(validate_kms_key_id("arn:aws:kms:us-east-1:123456789012:alias/my-key").is_ok());
    }

    #[test]
    fn validate_kms_key_id_alias() {
        assert!(validate_kms_key_id("alias/my-key").is_ok());
        assert!(validate_kms_key_id("alias/").is_err());
    }

    #[test]
    fn validate_kms_key_id_uuid() {
        assert!(validate_kms_key_id("1234abcd-12ab-34cd-56ef-1234567890ab").is_ok());
        assert!(validate_kms_key_id("not-a-uuid").is_err());
    }

    #[test]
    fn kms_key_id_carries_identity_and_validates_values() {
        let attr = kms_key_id();
        assert_refined_string_identity(&attr, "aws.kms.Key.Id");
        assert_string_attr_accepts_and_rejects(
            attr,
            "arn:aws:kms:us-east-1:123456789012:key/1234abcd-12ab-34cd-56ef-1234567890ab",
            "not-a-kms-key",
        );
    }

    // Service ARN tests

    #[test]
    fn validate_service_arn_valid() {
        assert!(
            validate_service_arn(
                "arn:aws:iam::123456789012:role/MyRole",
                "iam",
                Some("role/")
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_service_arn_wrong_service() {
        assert!(validate_service_arn("arn:aws:s3:::bucket", "iam", None).is_err());
    }

    #[test]
    fn validate_service_arn_wrong_prefix() {
        assert!(
            validate_service_arn(
                "arn:aws:iam::123456789012:user/MyUser",
                "iam",
                Some("role/")
            )
            .is_err()
        );
    }

    // --- validate_arn partition tests ---

    #[test]
    fn validate_arn_rejects_invalid_partition() {
        assert!(validate_arn("arn:xxx:iam::aws:policy/Foo").is_err());
        assert!(validate_arn("arn:invalid:s3:::bucket").is_err());
    }

    #[test]
    fn validate_arn_accepts_valid_partitions() {
        assert!(validate_arn("arn:aws:s3:::bucket").is_ok());
        assert!(validate_arn("arn:aws-cn:s3:::bucket").is_ok());
        assert!(validate_arn("arn:aws-us-gov:s3:::bucket").is_ok());
    }

    // --- IAM ARN validation tests ---

    #[test]
    fn validate_iam_arn_rejects_non_empty_region() {
        assert!(validate_iam_arn("arn:aws:iam:us-east-1:aws:policy/Foo", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_rejects_short_account_id() {
        assert!(validate_iam_arn("arn:aws:iam::1234:policy/Foo", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_rejects_non_digit_account() {
        assert!(validate_iam_arn("arn:aws:iam::aw:policy/Foo", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_accepts_aws_managed() {
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/AdministratorAccess", "policy/").is_ok());
    }

    #[test]
    fn validate_iam_arn_accepts_customer_managed() {
        assert!(validate_iam_arn("arn:aws:iam::123456789012:policy/MyPolicy", "policy/").is_ok());
    }

    #[test]
    fn validate_iam_arn_rejects_empty_resource_name() {
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_rejects_invalid_resource_chars() {
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/My Policy", "policy/").is_err());
        assert!(validate_iam_arn("arn:aws:iam::aws:policy/foo<bar>", "policy/").is_err());
    }

    #[test]
    fn validate_iam_arn_accepts_path_prefix() {
        assert!(
            validate_iam_arn(
                "arn:aws:iam::123456789012:role/service-role/MyRole",
                "role/"
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_iam_arn_error_says_iam_policy_arn() {
        let err = validate_iam_arn("arn:aws:iam:us-east-1:aws:policy/Foo", "policy/").unwrap_err();
        assert!(
            err.contains("IAM Policy ARN"),
            "Error should say 'IAM Policy ARN': {err}"
        );
        assert!(
            err.contains("arn:aws:iam:us-east-1:aws:policy/Foo"),
            "Error should include full ARN: {err}"
        );
    }

    #[test]
    fn validate_iam_arn_error_says_iam_role_arn() {
        let err = validate_iam_arn("arn:aws:iam:us-east-1:aws:role/Foo", "role/").unwrap_err();
        assert!(
            err.contains("IAM Role ARN"),
            "Error should say 'IAM Role ARN': {err}"
        );
    }

    #[test]
    fn iam_role_arn_carries_identity_and_validates_values() {
        let attr = iam_role_arn();
        assert_refined_string_identity(&attr, "aws.iam.Role.Arn");
        assert_string_attr_accepts_and_rejects(
            attr,
            "arn:aws:iam::123456789012:role/service-role/MyRole",
            "arn:aws:iam::123456789012:policy/MyPolicy",
        );
    }

    #[test]
    fn iam_policy_arn_carries_identity_and_validates_values() {
        let attr = iam_policy_arn();
        assert_refined_string_identity(&attr, "aws.iam.Policy.Arn");
        assert_string_attr_accepts_and_rejects(
            attr,
            "arn:aws:iam::123456789012:policy/MyPolicy",
            "arn:aws:iam::123456789012:role/MyRole",
        );
    }

    #[test]
    fn iam_oidc_provider_arn_carries_identity_and_validates_values() {
        let attr = iam_oidc_provider_arn();
        assert_refined_string_identity(&attr, "aws.iam.OidcProvider.Arn");
        assert_string_attr_accepts_and_rejects(
            attr,
            "arn:aws:iam::123456789012:oidc-provider/token.actions.githubusercontent.com",
            "arn:aws:iam::123456789012:role/MyRole",
        );
    }

    // UUID tests

    #[test]
    fn is_uuid_valid() {
        assert!(is_uuid("1234abcd-12ab-34cd-56ef-1234567890ab"));
    }

    #[test]
    fn is_uuid_invalid() {
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("1234abcd-12ab-34cd-56ef"));
        assert!(!is_uuid(""));
    }

    // IAM Policy Document tests

    #[test]
    fn validate_iam_policy_document_basic() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier("2012_10_17")),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::enum_identifier("allow")),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String(
                                        "sts:AssumeRole".to_string(),
                                    )),
                                ),
                                (
                                    "resource".to_string(),
                                    Value::Concrete(ConcreteValue::String("*".to_string())),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        assert!(validate_iam_policy_document(&doc).is_ok());
    }

    #[test]
    fn validate_iam_policy_document_invalid_version() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "version".to_string(),
                Value::Concrete(ConcreteValue::enum_identifier("2020_01_01")),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_iam_policy_document(&doc).is_err());
    }

    #[test]
    fn validate_iam_policy_document_invalid_effect() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "effect".to_string(),
                            Value::Concrete(ConcreteValue::enum_identifier("grant")),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_iam_policy_document(&doc).is_err());
    }

    #[test]
    fn iam_policy_document_type_validates() {
        let t = iam_policy_document();
        let valid_doc = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier("2012_10_17")),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::enum_identifier("deny")),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String("s3:*".to_string())),
                                ),
                                (
                                    "resource".to_string(),
                                    Value::Concrete(ConcreteValue::String("*".to_string())),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&valid_doc)
                .is_ok()
        );
    }

    #[test]
    fn iam_policy_document_principal_map_validates() {
        let t = iam_policy_document();
        // principal as a map: { service = "ec2.amazonaws.com" }
        let doc_with_principal_map = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier("2012_10_17")),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::enum_identifier("allow")),
                                ),
                                (
                                    "principal".to_string(),
                                    Value::Concrete(ConcreteValue::Map(
                                        vec![(
                                            "service".to_string(),
                                            Value::Concrete(ConcreteValue::String(
                                                "ec2.amazonaws.com".to_string(),
                                            )),
                                        )]
                                        .into_iter()
                                        .collect(),
                                    )),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String(
                                        "sts:AssumeRole".to_string(),
                                    )),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&doc_with_principal_map)
                .is_ok(),
            "principal as map (struct) should be valid: {:?}",
            carina_core::schema::Schema::flat(t.clone()).validate(&doc_with_principal_map)
        );
    }

    #[test]
    fn iam_policy_document_principal_string_validates() {
        let t = iam_policy_document();
        // principal as a string: "*"
        let doc_with_principal_string = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier("2012_10_17")),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                        ConcreteValue::Map(
                            vec![
                                (
                                    "effect".to_string(),
                                    Value::Concrete(ConcreteValue::enum_identifier("allow")),
                                ),
                                (
                                    "principal".to_string(),
                                    Value::Concrete(ConcreteValue::String("*".to_string())),
                                ),
                                (
                                    "action".to_string(),
                                    Value::Concrete(ConcreteValue::String(
                                        "sts:AssumeRole".to_string(),
                                    )),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&doc_with_principal_string)
                .is_ok(),
            "principal as string should be valid: {:?}",
            carina_core::schema::Schema::flat(t.clone()).validate(&doc_with_principal_string)
        );
    }

    #[test]
    fn transit_gateway_attachment_id_valid() {
        assert!(
            validate_prefixed_resource_id("tgw-attach-0123456789abcdef0", "tgw-attach").is_ok()
        );
    }

    #[test]
    fn transit_gateway_attachment_id_invalid() {
        assert!(validate_prefixed_resource_id("tgw-0123456789abcdef0", "tgw-attach").is_err());
    }

    #[test]
    fn flow_log_id_valid() {
        assert!(validate_prefixed_resource_id("fl-0123456789abcdef0", "fl").is_ok());
    }

    #[test]
    fn flow_log_id_invalid() {
        assert!(validate_prefixed_resource_id("fl-xyz", "fl").is_err());
    }

    #[test]
    fn ipam_id_valid() {
        assert!(validate_prefixed_resource_id("ipam-0123456789abcdef0", "ipam").is_ok());
    }

    #[test]
    fn ipam_id_invalid() {
        assert!(validate_prefixed_resource_id("ipam-pool-0123456789abcdef0", "ipam").is_err());
    }

    #[test]
    fn subnet_route_table_association_id_valid() {
        assert!(validate_prefixed_resource_id("rtbassoc-0123456789abcdef0", "rtbassoc").is_ok());
    }

    #[test]
    fn security_group_rule_id_valid() {
        assert!(validate_prefixed_resource_id("sgr-0123456789abcdef0", "sgr").is_ok());
    }

    #[test]
    fn security_group_rule_id_invalid() {
        assert!(validate_prefixed_resource_id("sg-0123456789abcdef0", "sgr").is_err());
    }

    #[test]
    fn iam_role_id_valid() {
        assert!(validate_iam_role_id("AROAEXAMPLEID123").is_ok());
        assert!(validate_iam_role_id("AROA1234567890ABCDEF").is_ok());
    }

    #[test]
    fn iam_role_id_invalid_prefix() {
        assert!(validate_iam_role_id("AIDA1234567890ABCDEF").is_err());
    }

    #[test]
    fn iam_role_id_invalid_empty_after_prefix() {
        assert!(validate_iam_role_id("AROA").is_err());
    }

    // SSO / Identity Center tests

    #[test]
    fn sso_principal_id_carries_identity_and_validates_values() {
        let attr = sso_principal_id();
        assert_refined_string_identity(&attr, "aws.sso.Principal.Id");
        assert_string_attr_accepts_and_rejects(
            attr,
            "1234567890-12345678-1234-1234-1234-1234567890ab",
            "",
        );
    }

    #[test]
    fn sso_instance_arn_carries_identity_and_validates_values() {
        let attr = sso_instance_arn();
        assert_refined_string_identity(&attr, "aws.sso.Instance.Arn");
        assert_string_attr_accepts_and_rejects(
            attr,
            "arn:aws:sso:::instance/ssoins-1234567890abcdef",
            "arn:aws:sso:::permissionSet/ssoins-1234567890abcdef/ps-1234567890abcdef",
        );
    }

    #[test]
    fn identity_store_id_carries_identity_and_validates_values() {
        let attr = identity_store_id();
        assert_refined_string_identity(&attr, "aws.identitystore.Store.Id");
        assert_string_attr_accepts_and_rejects(attr, "d-1234567890", "store-1234567890");
    }

    #[test]
    fn sso_permission_set_arn_carries_identity_and_validates_values() {
        let attr = sso_permission_set_arn();
        assert_refined_string_identity(&attr, "aws.sso.PermissionSet.Arn");
        assert_string_attr_accepts_and_rejects(
            attr,
            "arn:aws:sso:::permissionSet/ssoins-1234567890abcdef/ps-1234567890abcdef",
            "arn:aws:sso:::instance/ssoins-1234567890abcdef",
        );
    }

    // Region completion tests

    #[test]
    fn region_accepts_aws_format() {
        let region_type = aws_region();
        assert_eq!(string_enum_identity(&region_type), "aws.Region");
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "ap-northeast-1".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn region_accepts_dsl_format() {
        let region_type = aws_region();
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "aws.Region.ap_northeast_1".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn region_accepts_dsl_format_without_aws_prefix() {
        let region_type = aws_region();
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "Region.ap_northeast_1".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn region_accepts_bare_dsl_value() {
        let region_type = aws_region();
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "ap_northeast_1".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn region_rejects_invalid_region() {
        let region_type = aws_region();
        let result = carina_core::schema::Schema::flat(region_type.clone()).validate(
            &Value::Concrete(ConcreteValue::String("invalid-region".to_string())),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid region"));
        assert!(err.contains("ap-northeast-1")); // Should suggest valid regions
    }

    #[test]
    fn region_rejects_availability_zone() {
        let region_type = aws_region();
        // ap-northeast-1a is an AZ, not a region
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "ap-northeast-1a".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn region_validates_all_valid_regions() {
        let region_type = aws_region();
        for (region, _) in REGIONS {
            assert!(
                carina_core::schema::Schema::flat(region_type.clone())
                    .validate(&Value::Concrete(ConcreteValue::String(region.to_string())))
                    .is_ok(),
                "Region {} should be valid",
                region
            );
        }
    }

    #[test]
    fn region_rejects_wrong_namespace() {
        let region_type = aws_region();
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "gcp.Region.ap_northeast_1".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "aws.Location.ap_northeast_1".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "foo.bar.baz.ap_northeast_1".to_string()
                )))
                .is_err()
        );
        assert!(
            carina_core::schema::Schema::flat(region_type.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "Location.ap_northeast_1".to_string()
                )))
                .is_err()
        );
    }

    #[test]
    fn region_completions_generates_dsl_format() {
        let completions = region_completions("aws");
        assert_eq!(completions.len(), REGIONS.len());
        // Spot-check a few entries
        assert_eq!(completions[0].value, "aws.Region.af_south_1");
        assert_eq!(completions[0].description, "Africa (Cape Town)");
        let tokyo = completions
            .iter()
            .find(|c| c.value.contains("ap_northeast_1"))
            .unwrap();
        assert_eq!(tokyo.description, "Asia Pacific (Tokyo)");
    }

    #[test]
    fn region_completions_uses_provider_prefix() {
        let aws = region_completions("aws");
        let awscc = region_completions("awscc");
        assert!(aws[0].value.starts_with("aws.Region."));
        assert!(awscc[0].value.starts_with("awscc.Region."));
    }

    #[test]
    fn region_dsl_aliases_generates_api_to_dsl_pairs() {
        let aliases = region_dsl_aliases();
        assert_eq!(aliases.len(), REGIONS.len());
        assert!(aliases.contains(&("us-east-1".to_string(), "us_east_1".to_string())));
        assert!(aliases.contains(&("ap-northeast-1".to_string(), "ap_northeast_1".to_string())));
    }

    #[test]
    fn cloudfront_hosted_zone_id_carries_identity_and_validates_values() {
        let attr = cloudfront_hosted_zone_id();
        assert_eq!(string_enum_identity(&attr), "aws.cloudfront.HostedZoneId");
        let schema = carina_core::schema::Schema::flat(attr);
        assert!(
            schema
                .validate(&Value::Concrete(ConcreteValue::enum_identifier("global")))
                .is_ok()
        );
        assert!(
            schema
                .validate(&Value::Concrete(ConcreteValue::enum_identifier(
                    "Z0000000000000"
                )))
                .is_err()
        );
    }

    #[test]
    fn grantee_accepts_id_format() {
        let t = s3_grantee();
        assert_refined_string_identity(&t, "aws.s3.Grantee");
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "id=\"79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be\""
                        .to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_email_format() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "emailAddress=\"user@example.com\"".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_uri_format() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "uri=\"http://acs.amazonaws.com/groups/global/AllUsers\"".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_accepts_multiple_specs() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String(
                    "id=\"abc123\", emailAddress=\"user@example.com\"".to_string()
                )))
                .is_ok()
        );
    }

    #[test]
    fn grantee_rejects_empty_string() {
        let t = s3_grantee();
        assert!(
            carina_core::schema::Schema::flat(t.clone())
                .validate(&Value::Concrete(ConcreteValue::String("".to_string())))
                .is_err()
        );
    }

    #[test]
    fn grantee_rejects_invalid_prefix() {
        let t = s3_grantee();
        let result = carina_core::schema::Schema::flat(t.clone()).validate(&Value::Concrete(
            ConcreteValue::String("foo=\"bar\"".to_string()),
        ));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not match required pattern"));
    }

    #[test]
    fn validate_tags_map_detects_key_value_pattern() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Concrete(ConcreteValue::Map(
                [
                    (
                        "key".to_string(),
                        Value::Concrete(ConcreteValue::String("Project".to_string())),
                    ),
                    (
                        "value".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
        );
        assert!(validate_tags_map(&attrs).is_err());
    }

    #[test]
    fn validate_tags_map_case_insensitive() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Concrete(ConcreteValue::Map(
                [
                    (
                        "Key".to_string(),
                        Value::Concrete(ConcreteValue::String("Project".to_string())),
                    ),
                    (
                        "Value".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
        );
        assert!(validate_tags_map(&attrs).is_err());
    }

    #[test]
    fn validate_tags_map_normal_tags_ok() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(
            "tags".to_string(),
            Value::Concrete(ConcreteValue::Map(
                [
                    (
                        "Project".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                    (
                        "ManagedBy".to_string(),
                        Value::Concrete(ConcreteValue::String("carina".to_string())),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
        );
        assert!(validate_tags_map(&attrs).is_ok());
    }

    #[test]
    fn validate_tags_map_no_tags_ok() {
        let attrs = std::collections::HashMap::new();
        assert!(validate_tags_map(&attrs).is_ok());
    }

    // --- Condition operator tests ---

    #[test]
    fn condition_operator_to_aws_basic() {
        assert_eq!(
            condition_operator_to_aws("string_equals"),
            Some("StringEquals".to_string())
        );
        assert_eq!(
            condition_operator_to_aws("arn_like"),
            Some("ArnLike".to_string())
        );
        assert_eq!(condition_operator_to_aws("null"), Some("Null".to_string()));
    }

    #[test]
    fn condition_operator_to_aws_if_exists() {
        assert_eq!(
            condition_operator_to_aws("string_equals_if_exists"),
            Some("StringEqualsIfExists".to_string())
        );
        assert_eq!(
            condition_operator_to_aws("arn_like_if_exists"),
            Some("ArnLikeIfExists".to_string())
        );
    }

    #[test]
    fn condition_operator_to_aws_unknown() {
        assert_eq!(condition_operator_to_aws("unknown_op"), None);
        assert_eq!(condition_operator_to_aws("StringEquals"), None);
    }

    #[test]
    fn condition_operator_to_aws_for_all_values() {
        assert_eq!(
            condition_operator_to_aws("for_all_values_string_equals"),
            Some("ForAllValues:StringEquals".to_string())
        );
        assert_eq!(
            condition_operator_to_aws("for_any_value_string_like"),
            Some("ForAnyValue:StringLike".to_string())
        );
        // Any base operator should work with qualifiers
        assert_eq!(
            condition_operator_to_aws("for_all_values_numeric_equals"),
            Some("ForAllValues:NumericEquals".to_string())
        );
        // Combined qualifier + if_exists
        assert_eq!(
            condition_operator_to_aws("for_all_values_string_like_if_exists"),
            Some("ForAllValues:StringLikeIfExists".to_string())
        );
    }

    #[test]
    fn condition_operator_to_snake_roundtrip() {
        assert_eq!(
            condition_operator_to_snake("ForAllValues:NumericEquals"),
            Some("for_all_values_numeric_equals".to_string())
        );
        assert_eq!(
            condition_operator_to_snake("ForAnyValue:ArnLikeIfExists"),
            Some("for_any_value_arn_like_if_exists".to_string())
        );
    }

    #[test]
    fn condition_operator_typed_roundtrip() {
        // Type-level guarantee: every constructible ConditionOperator round-trips
        // through both snake/AWS Display forms. The schema's StringEnum, the
        // validator's "valid operators" list, and the conversion wrappers all
        // flow from `ConditionOperator::all()`, so this assertion forecloses
        // drift between them.
        for op in ConditionOperator::all() {
            let snake = op.to_snake();
            let aws = op.to_aws();
            assert_eq!(
                ConditionOperator::from_snake(&snake),
                Some(op),
                "snake {snake:?} did not round-trip"
            );
            assert_eq!(
                ConditionOperator::from_aws(&aws),
                Some(op),
                "aws {aws:?} did not round-trip"
            );
            // `Display` is the canonical AWS-wire spelling.
            assert_eq!(op.to_string(), aws);
        }
        // Spot-check a representative spelling, including the qualifier+if_exists
        // combination that issue #340 unblocked.
        let op = ConditionOperator {
            qualifier: Some(ConditionQualifier::ForAllValues),
            base: ConditionOperatorBase::StringLike,
            if_exists: true,
        };
        assert_eq!(op.to_snake(), "for_all_values_string_like_if_exists");
        assert_eq!(op.to_aws(), "ForAllValues:StringLikeIfExists");
    }

    #[test]
    fn condition_operator_total_count_matches_cross_product() {
        // 27 base × 3 qualifier options (None, ForAllValues, ForAnyValue) × 2
        // if_exists options = 162. If a new base operator is added the count
        // moves in lockstep, which is the property we want.
        let n_base = ConditionOperatorBase::ALL.len();
        let n_qualifier_options = 1 + ConditionQualifier::ALL.len();
        let expected = n_base * n_qualifier_options * 2;
        assert_eq!(ConditionOperator::all().count(), expected);
    }

    #[test]
    fn condition_type_string_enum_includes_qualifier_and_if_exists_variants() {
        // The schema's enum values must enumerate every snake_case spelling
        // that `condition_operator_to_aws` accepts — base, qualifier-prefixed,
        // `_if_exists` suffixed, and the combination — so that `validate` does
        // not reject inputs that the conversion layer already handles.
        let cond = condition_type();
        let carina_core::schema::Shape::Map { key, .. } =
            cond.shape_ref_free().expect("test schema is Ref-free")
        else {
            panic!("condition_type() should be a Map");
        };
        let carina_core::schema::Shape::Enum {
            values: Some(values),
            ..
        } = key.shape_ref_free().expect("test schema is Ref-free")
        else {
            panic!("condition_type() key should be an enum");
        };
        for expected in [
            "string_equals",
            "for_all_values_string_equals",
            "for_any_value_string_like",
            "string_equals_if_exists",
            "for_all_values_string_like_if_exists",
            "for_any_value_arn_like_if_exists",
            "null",
        ] {
            assert!(
                values.iter().any(|v| v == expected),
                "ConditionOperator StringEnum should include {expected:?}; got {values:?}"
            );
        }
        // Sanity: every value in the schema must round-trip through the
        // conversion layer, otherwise the two stay in drift.
        for v in values {
            assert!(
                condition_operator_to_aws(v).is_some(),
                "schema value {v:?} not accepted by condition_operator_to_aws"
            );
        }
    }

    #[test]
    fn validate_condition_operators_accepts_valid() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "string_equals".to_string(),
                                    Value::Concrete(ConcreteValue::Map(
                                        vec![(
                                            "aws:RequestedRegion".to_string(),
                                            Value::Concrete(ConcreteValue::String(
                                                "us-east-1".to_string(),
                                            )),
                                        )]
                                        .into_iter()
                                        .collect(),
                                    )),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_condition_operators(&doc).is_ok());
    }

    #[test]
    fn validate_condition_operators_rejects_pascal_case() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "StringEquals".to_string(),
                                    Value::Concrete(ConcreteValue::Map(indexmap::IndexMap::new())),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        let err = validate_condition_operators(&doc).unwrap_err();
        assert!(
            err.contains("StringEquals"),
            "Error should mention the invalid key: {err}"
        );
    }

    #[test]
    fn validate_condition_operators_rejects_unknown() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "foo_bar".to_string(),
                                    Value::Concrete(ConcreteValue::Map(indexmap::IndexMap::new())),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_condition_operators(&doc).is_err());
    }

    #[test]
    fn validate_condition_operators_accepts_if_exists() {
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![(
                "statement".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::Map(
                        vec![(
                            "condition".to_string(),
                            Value::Concrete(ConcreteValue::Map(
                                vec![(
                                    "string_equals_if_exists".to_string(),
                                    Value::Concrete(ConcreteValue::Map(indexmap::IndexMap::new())),
                                )]
                                .into_iter()
                                .collect(),
                            )),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        assert!(validate_condition_operators(&doc).is_ok());
    }

    #[test]
    fn validate_iam_policy_document_accepts_for_all_values_string_equals() {
        // Regression for the route53 cross-account delegation writer use case
        // (issue #340): `for_all_values_string_equals` is the only way to narrow
        // `route53:ChangeResourceRecordSets` to a specific record-name / type /
        // action set. The conversion layer accepts it; the schema must too.
        let condition_inner = Value::Concrete(ConcreteValue::Map(
            vec![(
                "route53:ChangeResourceRecordSetsRecordTypes".to_string(),
                Value::Concrete(ConcreteValue::List(vec![Value::Concrete(
                    ConcreteValue::String("NS".to_string()),
                )])),
            )]
            .into_iter()
            .collect(),
        ));
        let statement = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "effect".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier("allow")),
                ),
                (
                    "action".to_string(),
                    Value::Concrete(ConcreteValue::String(
                        "route53:ChangeResourceRecordSets".to_string(),
                    )),
                ),
                (
                    "resource".to_string(),
                    Value::Concrete(ConcreteValue::String(
                        "arn:aws:route53:::hostedzone/ABC".to_string(),
                    )),
                ),
                (
                    "condition".to_string(),
                    Value::Concrete(ConcreteValue::Map(
                        vec![("for_all_values_string_equals".to_string(), condition_inner)]
                            .into_iter()
                            .collect(),
                    )),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        let doc = Value::Concrete(ConcreteValue::Map(
            vec![
                (
                    "version".to_string(),
                    Value::Concrete(ConcreteValue::enum_identifier("2012_10_17")),
                ),
                (
                    "statement".to_string(),
                    Value::Concrete(ConcreteValue::List(vec![statement])),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        validate_iam_policy_document(&doc).expect(
            "for_all_values_string_equals must validate (schema must match conversion layer)",
        );
    }

    #[test]
    fn iam_policy_effect_is_string_enum() {
        let effect = super::iam_policy_effect();
        if let carina_core::schema::Shape::Enum {
            values: Some(values),
            identity,
            dsl_aliases,
            ..
        } = effect.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(identity.kind, "Effect");
            assert_eq!(values, &["Allow".to_string(), "Deny".to_string()]);
            assert_eq!(
                identity.dotted_prefix(),
                Some("aws.iam.PolicyDocument.Statement".to_string())
            );
            assert_eq!(
                dsl_aliases,
                &[
                    ("Allow".to_string(), "allow".to_string()),
                    ("Deny".to_string(), "deny".to_string()),
                ]
            );
        } else {
            panic!("expected enum");
        }
    }

    #[test]
    fn iam_policy_version_is_string_enum() {
        let version = super::iam_policy_version();
        if let carina_core::schema::Shape::Enum {
            values: Some(values),
            identity,
            dsl_aliases,
            ..
        } = version.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(identity.kind, "Version");
            assert_eq!(
                values,
                &["2012-10-17".to_string(), "2008-10-17".to_string()]
            );
            assert_eq!(
                identity.dotted_prefix(),
                Some("aws.iam.PolicyDocument".to_string())
            );
            assert_eq!(
                dsl_aliases,
                &[
                    ("2012-10-17".to_string(), "2012_10_17".to_string()),
                    ("2008-10-17".to_string(), "2008_10_17".to_string()),
                ]
            );
        } else {
            panic!("expected enum");
        }
    }

    /// Regression: `s3_sse_algorithm` must register DSL aliases for every
    /// value so core rewrites the DSL alias spelling (e.g. `aes256`) to
    /// the AWS API canonical (`AES256`) before the apply path forwards it
    /// to S3. Without aliases the literal alias string flows on the wire and
    /// `PutBucketEncryption` returns `MalformedXML`. See
    /// `carina-rs/carina-provider-aws#390`.
    #[test]
    fn s3_sse_algorithm_has_dsl_aliases() {
        let sse = super::s3_sse_algorithm();
        if let carina_core::schema::Shape::Enum {
            values: Some(values),
            identity,
            dsl_aliases,
            ..
        } = sse.shape_ref_free().expect("test schema is Ref-free")
        {
            assert_eq!(identity.kind, "SseAlgorithm");
            assert_eq!(
                values,
                &[
                    "AES256".to_string(),
                    "aws:kms".to_string(),
                    "aws:kms:dsse".to_string(),
                ]
            );
            assert_eq!(
                identity.dotted_prefix(),
                Some(
                    "aws.s3.BucketServerSideEncryptionConfiguration.SseRule.SseByDefault"
                        .to_string()
                )
            );
            // `dsl_enum_value` rewrites colon-separated AWS spellings so they
            // remain reachable as bare DSL identifiers. The `AES256 → aes256`
            // rewrite remains the load-bearing case for #390, and exhaustive
            // rows keep strict-DSL validation uniform across the variant set.
            assert_eq!(
                dsl_aliases,
                &[
                    ("AES256".to_string(), "aes256".to_string()),
                    ("aws:kms".to_string(), "aws_kms".to_string()),
                    ("aws:kms:dsse".to_string(), "aws_kms_dsse".to_string()),
                ]
            );
        } else {
            panic!("expected enum");
        }
    }

    /// Sibling regression flagged in #390: the `protocol` field of
    /// `RedirectAllRequestsTo` on `aws.s3.BucketWebsiteConfiguration`
    /// suffers the same missing-aliases bug. Latent today (no fixture
    /// exercises it) but the test pins the contract.
    #[test]
    fn s3_redirect_protocol_has_dsl_aliases() {
        let redirect = super::s3_redirect_all_requests_to();
        let mut budget = carina_core::schema::ShapeWalkBudget::new(8);
        let Some(fields) = redirect
            .struct_fields_ref_free_with_budget(&mut budget)
            .expect("test schema is Ref-free")
        else {
            panic!("expected Struct shape");
        };
        let protocol = fields
            .iter()
            .find(|f| f.name == "protocol")
            .expect("RedirectAllRequestsTo must have a `protocol` field")
            .field_type
            .clone();
        let carina_core::schema::Shape::Enum {
            identity,
            dsl_aliases,
            ..
        } = protocol.shape_ref_free().expect("test schema is Ref-free")
        else {
            panic!("expected protocol to be an enum");
        };
        assert_eq!(identity.kind, "Protocol");
        assert_eq!(
            dsl_aliases,
            &[
                ("http".to_string(), "http".to_string()),
                ("https".to_string(), "https".to_string()),
            ]
        );
    }
}
