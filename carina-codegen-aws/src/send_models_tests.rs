use super::*;

fn value(attribute_type: AttributeType) -> ValueTypeOverride {
    ValueTypeOverride {
        type_code: "unused()".into(),
        attribute_type,
        documentation: None,
        validated_input_constraints: Vec::new(),
    }
}

#[test]
fn multi_identity_string_slot_uses_string_send_model() {
    let model = carina_smithy::parse(r#"{"smithy":"2.0","shapes":{}}"#).unwrap();
    let member = carina_smithy::ShapeRef {
        target: "smithy.api#String".into(),
        traits: Default::default(),
    };
    let derived = string_members::derive(
        &model,
        &member,
        "test.MultiIdentity",
        "identifier",
        &[TypeOverride::Identities(&[
            AwsType::InternetGatewayId,
            AwsType::VpnGatewayId,
        ])],
    )
    .unwrap();
    assert!(matches!(
        derived.attribute_type.raw_shape(),
        RawShape::Union(_)
    ));
    assert_eq!(send_field_type(&derived), SendFieldType::String);
}

#[test]
fn nested_string_alternatives_use_string_send_model() {
    let ty = AttributeType::union(vec![
        AttributeType::string(),
        AttributeType::union(vec![
            AwsType::InternetGatewayId.attribute_type(),
            AwsType::KmsKeyReference.attribute_type(),
        ]),
    ]);
    assert_eq!(send_field_type(&value(ty)), SendFieldType::String);
}

#[test]
fn heterogeneous_or_empty_alternatives_keep_value_send_model() {
    for alternatives in [
        vec![AttributeType::string(), AttributeType::int()],
        vec![
            AttributeType::string(),
            AttributeType::list(AttributeType::string()),
        ],
        vec![],
    ] {
        assert_eq!(
            send_field_type(&value(AttributeType::union(alternatives))),
            SendFieldType::Value
        );
    }
}
