use std::process::Command;

#[test]
fn inspequte_exits_non_zero_on_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_inspequte"))
        .arg("--input")
        .arg("missing.class")
        .output()
        .expect("run inspequte");

    assert!(!output.status.success());
}
