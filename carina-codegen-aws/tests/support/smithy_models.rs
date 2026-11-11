/// Resolve the pinned fixture directory through the same helper as the generators.
/// A missing cache can be populated with `scripts/download-smithy-models.sh`.
fn smithy_model_dir() -> std::path::PathBuf {
    let helper =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/smithy-models-dir.sh");
    let output = std::process::Command::new("bash")
        .arg(helper)
        .output()
        .expect("run Smithy model path helper");
    assert!(
        output.status.success(),
        "Smithy model path helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = String::from_utf8(output.stdout).expect("UTF-8 Smithy model path");
    std::path::PathBuf::from(path.trim_end())
}
