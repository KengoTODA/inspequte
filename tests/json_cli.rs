use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;
use tempfile::tempdir;

fn run_inspequte(args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_inspequte"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().expect("spawn inspequte");
    if let Some(input) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin pipe")
            .write_all(input.as_bytes())
            .expect("write stdin");
    }
    child.wait_with_output().expect("collect output")
}

#[test]
fn json_stdin_scan_produces_sarif_output() {
    let temp_dir = tempdir().expect("temp dir");
    let request = format!(
        "{{\"command\":\"scan\",\"input\":[\"{}\"]}}",
        temp_dir.path().display()
    );

    let output = run_inspequte(&["--json", "-"], Some(&request));

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    let value: Value = serde_json::from_str(&stdout).expect("valid sarif JSON");
    assert_eq!(value["version"], "2.1.0");
}

#[test]
fn json_stdin_baseline_writes_output_file() {
    let temp_dir = tempdir().expect("temp dir");
    let baseline_path = temp_dir.path().join("inspequte.baseline.json");
    let request = format!(
        "{{\"command\":\"baseline\",\"input\":[\"{}\"],\"output\":\"{}\"}}",
        temp_dir.path().display(),
        baseline_path.display()
    );

    let output = run_inspequte(&["--json", "-"], Some(&request));

    assert!(output.status.success());
    assert!(baseline_path.exists());
    let baseline = fs::read_to_string(&baseline_path).expect("read baseline");
    let value: Value = serde_json::from_str(&baseline).expect("baseline JSON");
    assert_eq!(value["version"], 1);
}

#[test]
fn json_conflicts_with_legacy_flags() {
    let temp_dir = tempdir().expect("temp dir");
    let request = format!(
        "{{\"command\":\"scan\",\"input\":[\"{}\"]}}",
        temp_dir.path().display()
    );
    let output = run_inspequte(&["--json", &request, "--input", "."], None);

    assert!(!output.status.success());
}

#[test]
fn json_scan_and_legacy_scan_have_equivalent_results_and_rules() {
    let temp_dir = tempdir().expect("temp dir");
    let direct = run_inspequte(&["--input", temp_dir.path().to_str().expect("utf8")], None);
    assert!(direct.status.success());
    let direct_value: Value = serde_json::from_slice(&direct.stdout).expect("direct sarif");

    let request = format!(
        "{{\"command\":\"scan\",\"input\":[\"{}\"]}}",
        temp_dir.path().display()
    );
    let json_mode = run_inspequte(&["--json", &request], None);
    assert!(json_mode.status.success());
    let json_value: Value = serde_json::from_slice(&json_mode.stdout).expect("json sarif");

    assert_eq!(
        direct_value["runs"][0]["results"],
        json_value["runs"][0]["results"]
    );
    assert_eq!(
        direct_value["runs"][0]["tool"]["driver"]["rules"],
        json_value["runs"][0]["tool"]["driver"]["rules"]
    );
}
