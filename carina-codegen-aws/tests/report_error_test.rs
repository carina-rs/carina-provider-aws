use std::process::Command;

#[test]
fn report_failure_does_not_mask_codegen_error() {
    let cache = carina_codegen_aws::smithy_model_dir().join("iam.json");
    if !cache.exists() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let mut model: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cache).unwrap()).unwrap();
    let target =
        model["shapes"]["com.amazonaws.iam#CreateRoleRequest"]["members"]["Description"]["target"]
            .as_str()
            .unwrap()
            .to_owned();
    model["shapes"][&target]["traits"]["smithy.api#pattern"] =
        serde_json::json!("(?=untranslatable)");
    std::fs::write(
        temp.path().join("iam.json"),
        serde_json::to_vec(&model).unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_smithy-codegen"))
        .args(["--resource", "iam.Role", "--model-dir"])
        .arg(temp.path())
        .arg("--output-dir")
        .arg(temp.path().join("out"))
        .env(
            "CARINA_CODEGEN_CONSTRAINT_REPORT",
            temp.path().join("missing/report.json"),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("iam.Role.description") && error.contains("cannot translate"),
        "{error}"
    );
    assert!(
        error.contains("report") && error.contains("No such file"),
        "{error}"
    );
}
