use super::*;

include!("../tests/support/smithy_models.rs");

fn text(source: &str) -> (SmithyModel, carina_smithy::ShapeRef) {
    let model = carina_smithy::parse(
        &serde_json::json!({"smithy":"2.0","shapes":{
            "test#Text":{"type":"string","traits":{"smithy.api#pattern":source}},
            "test#Mode":{"type":"enum","members":{
                "A":{"target":"smithy.api#Unit","traits":{"smithy.api#enumValue":"A"}},
                "B":{"target":"smithy.api#Unit","traits":{"smithy.api#enumValue":"B"}}
            }}
        }})
        .to_string(),
    )
    .unwrap();
    (
        model,
        carina_smithy::ShapeRef {
            target: "test#Text".into(),
            traits: Default::default(),
        },
    )
}

#[test]
fn data_source_uses_its_declared_decoration() {
    let (model, _) = text(".*");
    let declared = DataSourceType::named(AwsType::IamRoleArn);
    let value = data_source_type(&model, "s3.Bucket", "arn", "Arn", &declared).unwrap();
    let RawShape::String {
        identity_layer:
            carina_core::schema::StringIdentityParts {
                identity: Some(identity),
                ..
            },
        ..
    } = value.attribute_type.raw_shape()
    else {
        panic!("identity")
    };
    assert_eq!(*identity, AwsType::IamRoleArn.identity());
    for source in resource_defs::sts_data_sources() {
        let resource_defs::DataSourceShape::Single { output_attributes } = source.shape else {
            panic!("single")
        };
        assert_eq!(
            output_attributes
                .iter()
                .find(|a| a.name == "arn")
                .unwrap()
                .type_code
                .decoration,
            Some(TypeOverride::Identity(AwsType::StsCallerIdentityArn))
        );
    }
}

#[test]
fn decoded_representation_keeps_its_tightening() {
    let (model, _) = text(".*");
    let source = DataSourceType {
        representation: Representation::Int,
        decoration: Some(TypeOverride::IntRange { min: 4, max: 8 }),
    };
    let value = data_source_type(&model, "test.Decoded", "count", "Count", &source).unwrap();
    assert!(
        matches!(
            value.attribute_type.raw_shape(),
            RawShape::Int {
                member_layer: carina_core::schema::IntMemberParts {
                    range: Some((Some(4), Some(8)))
                },
                ..
            }
        ),
        "declared tightening must not be ignored"
    );
}

#[test]
fn unused_bucket_resource_arn_helper_is_not_emitted() {
    let path = smithy_model_dir().join("s3.json");
    if !path.exists() {
        return;
    }
    let model = carina_smithy::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let resource = resource_defs::s3_resources()
        .into_iter()
        .find(|r| r.name == "s3.Bucket")
        .unwrap();
    let code = generate_resource(&resource, &model).unwrap();
    assert!(
        !code.contains("pub fn arn()"),
        "the resource helper has no repository callers; keep only the used data-source helper"
    );
}

#[test]
fn decorated_data_source_reference_uses_member_seam() {
    let model = carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).unwrap();
    let source = DataSourceType::named(AwsType::KmsKeyId);
    let result = data_source_type(&model, "logs.LogGroup", "kms_key_id", "KmsKeyId", &source);
    let error = result
        .err()
        .expect("a declared identity cannot be silently replaced by the reference identity");
    let message = format!("{error:#}");
    assert!(
        message.contains("logs.LogGroup.kms_key_id")
            && message.contains("reference slot cannot carry a String identity"),
        "{message}"
    );
}
