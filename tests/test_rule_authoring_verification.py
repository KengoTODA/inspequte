#!/usr/bin/env python3
"""Tests for structured rule verification results."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
MODULE_PATH = REPO_ROOT / "scripts" / "rule_authoring_verification.py"
SPEC = importlib.util.spec_from_file_location("rule_authoring_verification", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
verification = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verification)


class RuleAuthoringVerificationTest(unittest.TestCase):
    """Exercise structured result validation and Markdown generation."""

    def setUp(self) -> None:
        """Create a minimal verify-input directory with one evidence file."""
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.verify_root = Path(self.temporary_directory.name) / "verify-input"
        self.verify_root.mkdir()
        (self.verify_root / "diff.patch").write_text("patch\n", encoding="utf-8")
        self.manifest_path = self.verify_root / "manifest.json"
        self.manifest_path.write_text('{"attempt":1}\n', encoding="utf-8")

    def tearDown(self) -> None:
        """Release the temporary verification directory."""
        self.temporary_directory.cleanup()

    def manifest_hash(self) -> str:
        """Return the current test manifest hash."""
        return hashlib.sha256(self.manifest_path.read_bytes()).hexdigest()

    def no_go_result(self) -> dict[str, object]:
        """Return a valid implementation-defect result."""
        return {
            "schemaVersion": 1,
            "recommendation": "no_go",
            "reason": "implementation_defect",
            "implementationRetryable": True,
            "attempt": 1,
            "summary": "A required path is missing.",
            "evidenceManifestSha256": self.manifest_hash(),
            "findings": [
                {
                    "category": "spec_compliance",
                    "message": "A required case is not implemented.",
                    "evidence": [
                        {
                            "path": "diff.patch",
                            "detail": "The relevant match arm is absent.",
                        }
                    ],
                }
            ],
        }

    def test_valid_result_generates_required_markdown_sections(self) -> None:
        """The renderer derives the human report from normative JSON."""
        result = self.no_go_result()
        (self.verify_root / "verify-result.json").write_text(
            json.dumps(result), encoding="utf-8"
        )

        verification.finalize(self.verify_root)

        report = (self.verify_root / "verify-report.md").read_text(encoding="utf-8")
        self.assertIn("## Spec compliance findings", report)
        self.assertIn("## FP/noise risks", report)
        self.assertIn("## Determinism/stability risks", report)
        self.assertIn("## Performance and regression concerns", report)
        self.assertIn("## Recommendation (Go/No-Go)\n\nNo-Go", report)

    def test_go_with_a_defect_reason_is_rejected(self) -> None:
        """Recommendation and routing reason cannot contradict one another."""
        result = self.no_go_result()
        result["recommendation"] = "go"
        result["implementationRetryable"] = False

        with self.assertRaisesRegex(verification.VerificationResultError, "go requires"):
            verification.validate_result(self.verify_root, result)

    def test_result_bound_to_another_manifest_is_rejected(self) -> None:
        """A result cannot be reused for a different evidence manifest."""
        result = self.no_go_result()
        result["evidenceManifestSha256"] = "a" * 64

        with self.assertRaisesRegex(
            verification.VerificationResultError, "different manifest"
        ):
            verification.validate_result(self.verify_root, result)

    def test_missing_evidence_reference_is_rejected(self) -> None:
        """Every finding must cite an existing file inside verify-input."""
        result = self.no_go_result()
        findings = result["findings"]
        assert isinstance(findings, list)
        findings[0]["evidence"][0]["path"] = "reports/missing.txt"

        with self.assertRaisesRegex(verification.VerificationResultError, "does not exist"):
            verification.validate_result(self.verify_root, result)


if __name__ == "__main__":
    unittest.main()
