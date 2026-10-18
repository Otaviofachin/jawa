#[test]
fn prints_version() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_jawa"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "jawa 0.1.0");
}
