use std::collections::HashMap;

use carina_codegen_aws::resend_safety::{OVERRIDES, generate_provider_module};

#[test]
fn s3_delete_objects_requires_versioned_identifiers_and_hides_raw_builder() {
    let model = carina_smithy::parse(
        r#"{
            "smithy": "2.0",
            "shapes": {
                "test#S3": {
                    "type": "service",
                    "version": "1",
                    "operations": [{"target": "test#DeleteObjects"}]
                },
                "test#DeleteObjects": {"type": "operation"}
            }
        }"#,
    )
    .expect("valid S3 model");
    let decision = *OVERRIDES
        .iter()
        .find(|entry| entry.service == "s3" && entry.operation == "DeleteObjects")
        .expect("DeleteObjects decision");
    let generated = generate_provider_module(&HashMap::from([("s3", model)]), &[decision])
        .expect("generation succeeds");

    let start = generated
        .find("pub(crate) async fn delete_objects(")
        .expect("DeleteObjects must send internally without exposing its raw builder");
    let signature = generated[start..].split_once('{').unwrap().0;
    assert!(signature.contains("bucket: &str"), "{signature}");
    assert!(
        signature.contains("objects: Vec<crate::s3_delete::VersionedObjectId>"),
        "DeleteObjects must require a version ID for every identifier: {signature}"
    );
    assert!(!signature.contains("FluentBuilder"), "{signature}");
    assert!(!signature.contains("ObjectIdentifier"), "{signature}");
}
