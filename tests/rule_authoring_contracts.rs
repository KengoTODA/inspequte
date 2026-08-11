use serde_json::{Value, json};

const EVIDENCE_MANIFEST_SCHEMA: &str =
    include_str!("../schemas/rule-authoring/evidence-manifest.schema.json");
const VERIFICATION_RESULT_SCHEMA: &str =
    include_str!("../schemas/rule-authoring/verification-result.schema.json");

fn assert_valid(schema_text: &str, instance: &Value) {
    let schema: Value = serde_json::from_str(schema_text).expect("parse contract schema");
    let validator = jsonschema::validator_for(&schema).expect("compile contract schema");
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert!(errors.is_empty(), "schema errors:\n{}", errors.join("\n"));
}

fn assert_invalid(schema_text: &str, instance: &Value) {
    let schema: Value = serde_json::from_str(schema_text).expect("parse contract schema");
    let validator = jsonschema::validator_for(&schema).expect("compile contract schema");
    assert!(!validator.is_valid(instance));
}

#[test]
fn evidence_manifest_schema_accepts_a_bound_source_state() {
    let hash = "a".repeat(64);
    let commit = "b".repeat(40);
    let instance = json!({
        "schemaVersion": 1,
        "ruleId": "class_a_rule",
        "attempt": 1,
        "createdAt": "2026-08-11T00:00:00Z",
        "source": {
            "baseCommitSha": commit,
            "headCommitSha": "c".repeat(40),
            "treeSha": "d".repeat(40),
            "diffSha256": hash,
            "specSha256": "e".repeat(64)
        },
        "changedFiles": [{
            "path": "src/rules/class_a_rule/mod.rs",
            "status": "added",
            "sha256": "f".repeat(64)
        }],
        "reports": [{
            "command": "cargo test",
            "path": "reports/cargo-test.txt",
            "exitCode": 0,
            "sha256": "1".repeat(64)
        }],
        "tools": {
            "cargo": "cargo 1.90.0"
        }
    });

    assert_valid(EVIDENCE_MANIFEST_SCHEMA, &instance);
}

#[test]
fn verification_result_schema_enforces_routing_consistency() {
    let valid = json!({
        "schemaVersion": 1,
        "recommendation": "no_go",
        "reason": "implementation_defect",
        "implementationRetryable": true,
        "attempt": 1,
        "summary": "The implementation misses a required path.",
        "evidenceManifestSha256": "a".repeat(64),
        "findings": [{
            "category": "spec_compliance",
            "message": "A required case is not implemented.",
            "evidence": [{
                "path": "diff.patch",
                "detail": "The relevant match arm is absent."
            }]
        }]
    });
    assert_valid(VERIFICATION_RESULT_SCHEMA, &valid);

    let mut invalid = valid;
    invalid["implementationRetryable"] = json!(false);
    assert_invalid(VERIFICATION_RESULT_SCHEMA, &invalid);
}

#[test]
fn verification_result_schema_requires_none_for_go() {
    let instance = json!({
        "schemaVersion": 1,
        "recommendation": "go",
        "reason": "test_defect",
        "implementationRetryable": false,
        "attempt": 1,
        "summary": "Contradictory result.",
        "evidenceManifestSha256": "a".repeat(64),
        "findings": []
    });

    assert_invalid(VERIFICATION_RESULT_SCHEMA, &instance);
}

#[test]
fn verification_result_schema_requires_evidence_for_no_go() {
    let instance = json!({
        "schemaVersion": 1,
        "recommendation": "no_go",
        "reason": "implementation_defect",
        "implementationRetryable": true,
        "attempt": 1,
        "summary": "No evidence was provided.",
        "evidenceManifestSha256": "a".repeat(64),
        "findings": []
    });

    assert_invalid(VERIFICATION_RESULT_SCHEMA, &instance);
}
