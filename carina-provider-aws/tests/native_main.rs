#![cfg(not(target_arch = "wasm32"))]

#[test]
fn native_binary_exits_unsuccessfully() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_carina-provider-aws"))
        .output()
        .expect("run native provider binary");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no longer has a native provider"));
}
