use std::path::Path;
use std::process::Command;

fn generate(format: &str) -> Option<(tempfile::TempDir, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let models = root.join("carina-provider-aws/tests/fixtures/smithy");
    if !models.join("sqs.json").exists() {
        eprintln!(
            "Skipping SQS generation: run scripts/download-smithy-models.sh to populate the model cache"
        );
        return None;
    }
    let output = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_smithy-codegen"))
        .arg("--model-dir")
        .arg(models)
        .arg("--output-dir")
        .arg(output.path())
        .args(["--resource", "sqs.Queue", "--format", format])
        .arg("--examples-dir")
        .arg(root.join("examples"))
        .output()
        .expect("run native codegen against cached models");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let suffix = if format == "markdown" { "md" } else { "rs" };
    let generated =
        std::fs::read_to_string(output.path().join(format!("sqs/queue.{suffix}"))).unwrap();
    Some((output, generated))
}

#[test]
fn sqs_codegen_rust_uses_exact_value_paths_and_preserves_carriers() {
    let Some((_output, generated)) = generate("rust") else {
        return;
    };
    for expected in [
        r"^(?:[A-Za-z0-9_-]{1,80}|[A-Za-z0-9_-]{1,75}\\.fifo)\\z",
        "carina_aws_types::with_int_range(AttributeType::int(), Some(0), Some(43200))",
        "carina_aws_types::with_int_range(AttributeType::int(), Some(60), Some(1209600))",
        "carina_aws_types::with_int_range(AttributeType::int(), Some(0), Some(900))",
        "carina_aws_types::with_int_range(AttributeType::int(), Some(1024), Some(1048576))",
        "carina_aws_types::with_int_range(AttributeType::int(), Some(0), Some(20))",
        "carina_aws_types::with_int_range(AttributeType::int(), Some(60), Some(86400))",
        "carina_aws_types::AwsType::SqsQueueArn.attribute_type()",
        "carina_aws_types::AwsType::SqsQueueDeduplicationScope.attribute_type()",
        "carina_aws_types::AwsType::SqsQueueFifoThroughputLimit.attribute_type()",
        // Native/protocol schema tests check the helper leaves' exact types
        // and bound edges. Generated schemas must use those constructors.
        "AttributeSchema::optional(\"redrive_policy\", super::sqs_redrive_policy())",
        "AttributeSchema::optional(\"redrive_allow_policy\", super::sqs_redrive_allow_policy())",
    ] {
        assert!(
            generated.contains(expected),
            "missing {expected}: {generated}"
        );
    }
    for field in [
        "approximate_number_of_messages",
        "approximate_number_of_messages_delayed",
        "approximate_number_of_messages_not_visible",
        "created_timestamp",
        "last_modified_timestamp",
    ] {
        assert!(
            generated.contains(&format!(
                "AttributeSchema::provider_populated(\"{field}\", AttributeType::int())"
            )),
            "{field}"
        );
    }
}

#[test]
fn sqs_codegen_docs_show_ranges_names_arns_and_dsl_enum_spellings() {
    let Some((_output, generated)) = generate("markdown") else {
        return;
    };
    for expected in [
        "`Int (member constraints: range: 0..=43200)`",
        "`Int (member constraints: range: 60..=1209600)`",
        "`Int (member constraints: range: 0..=900)`",
        "`Int (member constraints: range: 1024..=1048576)`",
        "`Int (member constraints: range: 0..=20)`",
        "`Int (member constraints: range: 60..=86400)`",
        "`Int (range: 1..=1000)`",
        "| `max_receive_count` | `Int (range: 1..=1000)` |",
        "| `dead_letter_target_arn` | `SqsQueueArn (identity constraints: pattern:",
        "| `source_queue_arns` | List\\<`SqsQueueArn (identity constraints: pattern:",
        "`String (member constraints: len: 1..=80)`",
        "| `queue_name` | Create only | `String (pattern: /^(?:[A-Za-z0-9_-]{1,80}\\|[A-Za-z0-9_-]{1,75}\\.fifo)\\z/ (Full))` |",
        "SqsQueueArn",
        "message_group",
        "per_queue",
        "per_message_group_id",
        "aws.sqs.Queue.DeduplicationScope",
        "aws.sqs.Queue.FifoThroughputLimit",
        "Defaults to 1,048,576 (1 MiB)",
    ] {
        assert!(
            generated.contains(expected),
            "missing {expected}: {generated}"
        );
    }
    assert!(!generated.contains("Range 1,024–262,144"));
}
