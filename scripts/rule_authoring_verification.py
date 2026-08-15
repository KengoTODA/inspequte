#!/usr/bin/env python3
"""Validate a rule verification result and render its Markdown summary."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
RECOMMENDATIONS = {"go", "no_go"}
REASONS = {
    "none",
    "implementation_defect",
    "test_defect",
    "spec_ambiguity",
    "suspected_false_positive",
    "stale_or_missing_evidence",
    "infrastructure_failure",
}
RETRYABLE_REASONS = {"implementation_defect", "test_defect"}
CATEGORY_HEADINGS = (
    ("spec_compliance", "Spec compliance findings"),
    ("false_positive_risk", "FP/noise risks"),
    ("determinism_stability", "Determinism/stability risks"),
    ("performance_regression", "Performance and regression concerns"),
)
ALLOWED_ROOT_FILES = {
    "spec.md",
    "diff.patch",
    "changed-files.txt",
    "deleted-files.txt",
}


class VerificationResultError(Exception):
    """Raised when a verification result violates its contract."""


def sha256_file(path: Path) -> str:
    """Return the lowercase SHA-256 digest for a file."""
    digest = hashlib.sha256()
    with path.open("rb") as reader:
        for block in iter(lambda: reader.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_json(path: Path) -> Any:
    """Load one JSON document with a verification-oriented error."""
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise VerificationResultError(f"cannot load JSON {path}: {error}") from error


def require_keys(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    """Require an object containing exactly the named keys."""
    if not isinstance(value, dict):
        raise VerificationResultError(f"{label} must be an object")
    actual = set(value)
    if actual != keys:
        raise VerificationResultError(
            f"{label} keys differ: missing={sorted(keys - actual)}, "
            f"unexpected={sorted(actual - keys)}"
        )
    return value


def require_text(value: Any, label: str) -> str:
    """Require non-empty text and return it unchanged."""
    if not isinstance(value, str) or not value.strip():
        raise VerificationResultError(f"{label} must be non-empty text")
    return value


def evidence_path(verify_root: Path, value: Any, label: str) -> str:
    """Require an allowed evidence path that exists inside verify-input."""
    path_text = require_text(value, label)
    if "\n" in path_text or "\r" in path_text or "`" in path_text:
        raise VerificationResultError(f"{label} contains unsupported characters")
    relative_path = Path(path_text)
    if relative_path.is_absolute() or ".." in relative_path.parts:
        raise VerificationResultError(f"{label} must remain inside verify-input")
    if path_text not in ALLOWED_ROOT_FILES and not (
        path_text.startswith("changes/") or path_text.startswith("reports/")
    ):
        raise VerificationResultError(f"{label} is not an allowed evidence path")
    resolved_root = verify_root.resolve()
    resolved_path = (verify_root / relative_path).resolve()
    if resolved_root not in resolved_path.parents:
        raise VerificationResultError(f"{label} escapes verify-input")
    if not resolved_path.is_file():
        raise VerificationResultError(f"{label} does not exist: {path_text}")
    return path_text


def validate_result(verify_root: Path, raw_result: Any) -> dict[str, Any]:
    """Validate result shape, routing semantics, and evidence identity."""
    result = require_keys(
        raw_result,
        {
            "schemaVersion",
            "recommendation",
            "reason",
            "implementationRetryable",
            "attempt",
            "summary",
            "evidenceManifestSha256",
            "findings",
        },
        "verification result",
    )
    if result["schemaVersion"] != SCHEMA_VERSION:
        raise VerificationResultError("unsupported verification result schemaVersion")
    if result["recommendation"] not in RECOMMENDATIONS:
        raise VerificationResultError("verification recommendation is invalid")
    if result["reason"] not in REASONS:
        raise VerificationResultError("verification reason is invalid")
    if type(result["implementationRetryable"]) is not bool:
        raise VerificationResultError("implementationRetryable must be a boolean")
    if type(result["attempt"]) is not int or not 1 <= result["attempt"] <= 3:
        raise VerificationResultError("verification attempt is invalid")
    require_text(result["summary"], "verification summary")
    digest = result["evidenceManifestSha256"]
    if not isinstance(digest, str) or SHA256_PATTERN.fullmatch(digest) is None:
        raise VerificationResultError("evidenceManifestSha256 is invalid")
    if not isinstance(result["findings"], list):
        raise VerificationResultError("verification findings must be an array")

    if result["recommendation"] == "go":
        if result["reason"] != "none" or result["implementationRetryable"]:
            raise VerificationResultError("go requires reason none and no implementation retry")
        if result["findings"]:
            raise VerificationResultError("go requires no findings")
    else:
        if result["reason"] == "none":
            raise VerificationResultError("no_go requires a concrete reason")
        expected_retryable = result["reason"] in RETRYABLE_REASONS
        if result["implementationRetryable"] is not expected_retryable:
            raise VerificationResultError(
                "implementationRetryable contradicts the verification reason"
            )
        if not result["findings"]:
            raise VerificationResultError("no_go requires at least one finding")

    categories = {category for category, _ in CATEGORY_HEADINGS}
    for finding_index, raw_finding in enumerate(result["findings"]):
        finding = require_keys(
            raw_finding,
            {"category", "message", "evidence"},
            f"finding {finding_index}",
        )
        if finding["category"] not in categories:
            raise VerificationResultError(f"finding {finding_index} category is invalid")
        require_text(finding["message"], f"finding {finding_index} message")
        if not isinstance(finding["evidence"], list) or not finding["evidence"]:
            raise VerificationResultError(
                f"finding {finding_index} must cite at least one evidence item"
            )
        for evidence_index, raw_evidence in enumerate(finding["evidence"]):
            item = require_keys(
                raw_evidence,
                {"path", "detail"},
                f"finding {finding_index} evidence {evidence_index}",
            )
            evidence_path(
                verify_root,
                item["path"],
                f"finding {finding_index} evidence {evidence_index} path",
            )
            require_text(
                item["detail"],
                f"finding {finding_index} evidence {evidence_index} detail",
            )

    manifest_path = verify_root / "manifest.json"
    manifest = load_json(manifest_path)
    if not isinstance(manifest, dict) or type(manifest.get("attempt")) is not int:
        raise VerificationResultError("manifest attempt is missing or invalid")
    if result["attempt"] != manifest["attempt"]:
        raise VerificationResultError("verification attempt does not match manifest")
    if digest != sha256_file(manifest_path):
        raise VerificationResultError("verification result cites a different manifest")
    return result


def compact_markdown_text(value: str) -> str:
    """Collapse untrusted result prose to one stable Markdown-safe line."""
    return " ".join(value.replace("`", "\\`").split())


def render_report(result: dict[str, Any]) -> str:
    """Render the normative JSON result as a deterministic Markdown report."""
    lines = [
        "# Rule verification report",
        "",
        f"- Attempt: {result['attempt']}",
        f"- Reason: `{result['reason']}`",
        f"- Evidence manifest SHA-256: `{result['evidenceManifestSha256']}`",
        f"- Summary: {compact_markdown_text(result['summary'])}",
    ]
    for category, heading in CATEGORY_HEADINGS:
        lines.extend(("", f"## {heading}", ""))
        findings = [
            finding for finding in result["findings"] if finding["category"] == category
        ]
        if not findings:
            lines.append("No findings.")
            continue
        for finding in findings:
            lines.append(f"- {compact_markdown_text(finding['message'])}")
            for item in finding["evidence"]:
                path = compact_markdown_text(item["path"])
                detail = compact_markdown_text(item["detail"])
                lines.append(f"  - Evidence: `{path}` — {detail}")
    recommendation = "Go" if result["recommendation"] == "go" else "No-Go"
    lines.extend(("", "## Recommendation (Go/No-Go)", "", recommendation, ""))
    return "\n".join(lines)


def finalize(verify_root: Path) -> None:
    """Validate verify-result.json and replace the generated Markdown report."""
    result_path = verify_root / "verify-result.json"
    result = validate_result(verify_root, load_json(result_path))
    report_path = verify_root / "verify-report.md"
    report_path.write_text(render_report(result), encoding="utf-8")
    print(f"Validated {result_path} and generated {report_path}.")


def main() -> int:
    """Run verification-result finalization."""
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("finalize",))
    parser.add_argument(
        "--verify-root",
        default=str(Path(__file__).resolve().parent.parent / "verify-input"),
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args()
    try:
        finalize(Path(args.verify_root).resolve())
    except VerificationResultError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
