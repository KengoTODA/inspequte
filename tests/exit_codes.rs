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

#[test]
fn inspequte_exits_non_zero_on_unknown_rule_in_exclude() {
    let output = Command::new(env!("CARGO_BIN_EXE_inspequte"))
        .args(["--input", "nonexistent-dir", "--exclude", "UNKNOWN_RULE"])
        .output()
        .expect("run inspequte");

    assert!(!output.status.success());
}

#[test]
fn inspequte_exits_non_zero_on_unknown_rule_in_only() {
    let output = Command::new(env!("CARGO_BIN_EXE_inspequte"))
        .args(["--input", "nonexistent-dir", "--only", "UNKNOWN_RULE"])
        .output()
        .expect("run inspequte");

    assert!(!output.status.success());
}

#[test]
fn inspequte_exits_non_zero_on_conflicting_filter_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_inspequte"))
        .args([
            "--input",
            "nonexistent-dir",
            "--only",
            "ARRAY_EQUALS",
            "--exclude",
            "ARRAY_EQUALS",
        ])
        .output()
        .expect("run inspequte");

    assert!(!output.status.success());
}
