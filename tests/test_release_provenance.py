"""Exercise release guards without network, signing or publication."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHA = "a" * 40
TARGETS = (
    "amd64-unknown-linux-gnu.tar.gz",
    "arm64-unknown-linux-gnu.tar.gz",
    "arm64-apple-darwin.tar.gz",
    "amd64-apple-darwin.tar.gz",
    "amd64-pc-windows-msvc.zip",
)


class ReleaseVerificationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.log = self.directory / "calls.jsonl"
        gh = self.directory / "gh"
        gh.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "with open(os.environ['VERIFY_LOG'], 'a') as f:\n"
            "    f.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            "sys.exit(1 if Path(sys.argv[3]).read_bytes() == b'tampered' else 0)\n"
        )
        gh.chmod(0o755)
        self.env = dict(
            os.environ,
            PATH=f"{self.directory}{os.pathsep}{os.environ['PATH']}",
            GITHUB_REPOSITORY="KengoTODA/inspequte",
            VERIFY_LOG=str(self.log),
        )
        self.assets = self.directory / "assets"
        self.assets.mkdir()
        for target in TARGETS:
            (self.assets / f"inspequte-inspequte-v1.0.0-{target}").write_bytes(b"built")

    def verify(self):
        return subprocess.run(
            ["bash", str(ROOT / "scripts/verify-cli-release.sh"),
             str(self.assets), "inspequte-v1.0.0", SHA],
            env=self.env, capture_output=True, text=True,
        )

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_verifies_complete_inventory_with_strict_identity(self):
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 5)
        for call in self.calls():
            self.assertEqual(call[:2], ["attestation", "verify"])
            self.assertEqual(call[call.index("--repo") + 1], "KengoTODA/inspequte")
            self.assertEqual(call[call.index("--signer-workflow") + 1],
                             "KengoTODA/inspequte/.github/workflows/build-cli-release.yml")
            self.assertEqual(call[call.index("--source-digest") + 1], SHA)
            self.assertEqual(call[call.index("--signer-digest") + 1], SHA)
            self.assertIn("--deny-self-hosted-runners", call)

    def test_missing_archive_fails_before_verification(self):
        next(self.assets.iterdir()).unlink()
        self.assertNotEqual(self.verify().returncode, 0)
        self.assertFalse(self.log.exists())

    def test_extra_archive_fails_before_verification(self):
        (self.assets / "unexpected.zip").write_bytes(b"built")
        self.assertNotEqual(self.verify().returncode, 0)
        self.assertFalse(self.log.exists())

    def test_tampered_archive_stops_on_verifier_failure(self):
        artifact = self.assets / f"inspequte-inspequte-v1.0.0-{TARGETS[0]}"
        artifact.write_bytes(b"tampered")
        self.assertNotEqual(self.verify().returncode, 0)
        self.assertEqual(len(self.calls()), 1)

    def test_symlink_is_rejected(self):
        artifact = self.assets / f"inspequte-inspequte-v1.0.0-{TARGETS[0]}"
        artifact.unlink()
        artifact.symlink_to(self.assets / f"inspequte-inspequte-v1.0.0-{TARGETS[1]}")
        self.assertNotEqual(self.verify().returncode, 0)
        self.assertFalse(self.log.exists())


class ReleaseSourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repository = Path(self.temp.name)
        self.git("init", "--quiet")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Release guard test")
        self.git("commit", "--allow-empty", "-m", "first")
        self.first = self.git("rev-parse", "HEAD").strip()
        self.git("tag", "inspequte-v1.0.0")
        self.git("tag", "gradle-plugin-v1.0.0")
        self.git("remote", "add", "origin", str(self.repository))

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repository, text=True,
                                       stderr=subprocess.DEVNULL)

    def validate(self, tag="inspequte-v1.0.0", sha=None, trigger=None, component="cli"):
        return subprocess.run(
            ["bash", str(ROOT / "scripts/validate-release-source.sh"),
             component, tag, sha or self.first],
            cwd=self.repository, env=dict(os.environ, GITHUB_SHA=trigger or self.first),
            capture_output=True, text=True,
        )

    def test_accepts_matching_cli_and_plugin_tags(self):
        self.assertEqual(self.validate().returncode, 0)
        self.assertEqual(self.validate("gradle-plugin-v1.0.0", component="gradle").returncode, 0)

    def test_rejects_release_sha_different_from_trigger(self):
        self.assertNotEqual(self.validate(trigger=SHA).returncode, 0)

    def test_rejects_tag_on_different_commit(self):
        self.git("commit", "--allow-empty", "-m", "second")
        second = self.git("rev-parse", "HEAD").strip()
        self.assertNotEqual(self.validate(sha=second, trigger=second).returncode, 0)

    def test_rejects_checkout_different_from_trigger(self):
        self.git("commit", "--allow-empty", "-m", "second")
        self.assertNotEqual(self.validate().returncode, 0)

    def test_rejects_malformed_tag(self):
        self.assertNotEqual(self.validate(tag="inspequte-v1.0.0;echo injected").returncode, 0)


if __name__ == "__main__":
    unittest.main()
