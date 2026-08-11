#!/usr/bin/env python3
"""Tests for source-bound rule-authoring evidence."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
MODULE_PATH = REPO_ROOT / "scripts" / "rule_authoring_evidence.py"
SPEC = importlib.util.spec_from_file_location("rule_authoring_evidence", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
evidence = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evidence)


class RuleAuthoringEvidenceTest(unittest.TestCase):
    """Exercise manifest creation and stale-evidence rejection in a temporary repo."""

    def setUp(self) -> None:
        """Create a minimal committed rule and one uncommitted implementation change."""
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.repo_root = Path(self.temporary_directory.name)
        self.git("init", "--quiet")
        self.git("config", "user.name", "Test User")
        self.git("config", "user.email", "test@example.invalid")
        rule_root = self.repo_root / "src" / "rules" / "class_a_rule"
        rule_root.mkdir(parents=True)
        (self.repo_root / ".gitignore").write_text(
            "verify-input/\n", encoding="utf-8"
        )
        (rule_root / "spec.md").write_text("# Contract\n", encoding="utf-8")
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "test: add contract")
        (rule_root / "mod.rs").write_text("// implementation\n", encoding="utf-8")

    def tearDown(self) -> None:
        """Release the temporary repository."""
        self.temporary_directory.cleanup()

    def git(self, *args: str) -> None:
        """Run Git in the temporary repository."""
        subprocess.run(
            ["git", "-C", str(self.repo_root), *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def write_reports(self) -> None:
        """Create the three fixed command reports and their exit codes."""
        reports = self.repo_root / "verify-input" / "reports"
        (reports / "cargo-build.txt").write_text("build ok\n", encoding="utf-8")
        (reports / "cargo-test.txt").write_text("test ok\n", encoding="utf-8")
        (reports / "cargo-audit.sarif").write_text("{}\n", encoding="utf-8")
        (reports / "cargo-audit.stderr.txt").write_text("", encoding="utf-8")
        (reports / "command-status.txt").write_text(
            "cargo build exit code: 0\n"
            "cargo test exit code: 0\n"
            "cargo audit exit code: 0\n",
            encoding="utf-8",
        )

    def test_manifest_binds_uncommitted_source_and_reports(self) -> None:
        """A valid manifest covers the working tree without changing the real index."""
        evidence.prepare(self.repo_root, "class_a_rule", "", 1)
        self.write_reports()
        evidence.create_manifest(self.repo_root)
        evidence.validate(self.repo_root)

        manifest = json.loads(
            (self.repo_root / "verify-input" / "manifest.json").read_text(
                encoding="utf-8"
            )
        )
        self.assertEqual(manifest["attempt"], 1)
        self.assertEqual(
            manifest["changedFiles"][0]["path"],
            "src/rules/class_a_rule/mod.rs",
        )
        self.assertEqual(self.git_status(), "?? src/rules/class_a_rule/mod.rs")

    def test_tampered_report_is_rejected(self) -> None:
        """Report content changed after manifest creation is stale evidence."""
        evidence.prepare(self.repo_root, "class_a_rule", "", 1)
        self.write_reports()
        evidence.create_manifest(self.repo_root)
        report = self.repo_root / "verify-input" / "reports" / "cargo-test.txt"
        report.write_text("different\n", encoding="utf-8")

        with self.assertRaisesRegex(evidence.EvidenceError, "report evidence"):
            evidence.validate(self.repo_root)

    def test_source_change_after_prepare_is_rejected(self) -> None:
        """Source changes made after preparation invalidate the captured tree."""
        evidence.prepare(self.repo_root, "class_a_rule", "", 1)
        self.write_reports()
        evidence.create_manifest(self.repo_root)
        implementation = self.repo_root / "src" / "rules" / "class_a_rule" / "mod.rs"
        implementation.write_text("// changed later\n", encoding="utf-8")

        with self.assertRaisesRegex(evidence.EvidenceError, "source tree changed"):
            evidence.validate(self.repo_root)

    def git_status(self) -> str:
        """Return the compact Git status of the temporary repository."""
        result = subprocess.run(
            ["git", "-C", str(self.repo_root), "status", "--short"],
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        )
        return result.stdout.strip()


if __name__ == "__main__":
    unittest.main()
