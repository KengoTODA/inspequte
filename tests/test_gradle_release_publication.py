"""Validate the actual Portal pre-upload hook with restored and tampered files.

Run after Gradle's prepareReleaseArtifacts task. All Portal calls use
--validate-only; gh is replaced by a digest-checking stub, never real signing.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
PLUGIN = ROOT / "gradle-plugin"


class GradlePublicationTests(unittest.TestCase):
    def test_restored_bytes_are_verified_and_tampering_stops_upload(self):
        prepared = PLUGIN / "build/release-artifacts"
        files = [path for path in prepared.rglob("*") if path.is_file()]
        self.assertTrue(files, "Run prepareReleaseArtifacts before this test")
        self.assertTrue(any(path.suffix == ".jar" for path in files))
        self.assertTrue(any(path.name == "pom-default.xml" for path in files))
        hashes = {str(path.relative_to(prepared)): hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in files}
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            manifest = work / "hashes.json"
            manifest.write_text(json.dumps(hashes))
            log = work / "verified.jsonl"
            gh = work / "gh"
            gh.write_text(
                "#!/usr/bin/env python3\n"
                "import hashlib, json, os, sys\n"
                "from pathlib import Path\n"
                "args = sys.argv[1:]\n"
                "assert args[:2] == ['attestation', 'verify']\n"
                "assert args[args.index('--signer-workflow') + 1] == 'KengoTODA/inspequte/.github/workflows/build-gradle-release.yml'\n"
                "assert args[args.index('--source-digest') + 1] == 'a' * 40\n"
                "assert args[args.index('--signer-digest') + 1] == 'a' * 40\n"
                "assert '--deny-self-hosted-runners' in args\n"
                "file = Path(args[2])\n"
                "relative = str(file.relative_to(os.environ['PUBLICATION_BUILD']))\n"
                "hashes = json.loads(Path(os.environ['PUBLICATION_HASHES']).read_text())\n"
                "if hashlib.sha256(file.read_bytes()).hexdigest() != hashes.get(relative):\n"
                "    sys.exit(1)\n"
                "with open(os.environ['PUBLICATION_LOG'], 'a') as output:\n"
                "    output.write(json.dumps(relative) + '\\n')\n"
            )
            gh.chmod(0o755)
            env = dict(os.environ, PATH=f"{work}{os.pathsep}{os.environ['PATH']}",
                       GITHUB_REPOSITORY="KengoTODA/inspequte", GITHUB_SHA="a" * 40,
                       PUBLICATION_BUILD=str(PLUGIN / "build"),
                       PUBLICATION_HASHES=str(manifest), PUBLICATION_LOG=str(log))

            # Simulate a fresh publishing runner: restore only the files that
            # were attested, with no compilation outputs or Gradle task history.
            for directory in ("libs", "publications", "classes", "tmp", "kotlin"):
                shutil.rmtree(PLUGIN / "build" / directory, ignore_errors=True)
            shutil.copytree(prepared, PLUGIN / "build", dirs_exist_ok=True)
            command = ["./gradlew", "publishPlugins", "--validate-only", "--no-daemon",
                       "--no-configuration-cache", "-PverifyRelease"]
            result = subprocess.run(command, cwd=PLUGIN, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            verified = {json.loads(line) for line in log.read_text().splitlines()}
            self.assertEqual(verified, set(hashes))
            for relative, digest in hashes.items():
                self.assertEqual(hashlib.sha256((PLUGIN / "build" / relative).read_bytes()).hexdigest(), digest)

            jar = next((PLUGIN / "build/libs").glob("*.jar"))
            original = jar.read_bytes()
            try:
                jar.write_bytes(original + b"tampered")
                result = subprocess.run(command, cwd=PLUGIN, env=env, capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Release attestation verification failed", result.stdout + result.stderr)
            finally:
                jar.write_bytes(original)


if __name__ == "__main__":
    unittest.main()
