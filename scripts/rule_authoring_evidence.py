#!/usr/bin/env python3
"""Create and validate source-bound evidence for rule authoring."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCRIPT_VERSION = 1
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
GIT_ID_PATTERN = re.compile(r"^[0-9a-f]{40}(?:[0-9a-f]{24})?$")
RULE_ID_PATTERN = re.compile(r"^[a-z][a-z0-9_]*$")
STATUS_NAMES = {
    "A": "added",
    "C": "copied",
    "D": "deleted",
    "M": "modified",
    "R": "renamed",
    "T": "type_changed",
}
REPORT_OUTPUTS = (
    ("cargo build", "reports/cargo-build.txt"),
    ("cargo test", "reports/cargo-test.txt"),
    ("cargo audit", "reports/cargo-audit.sarif"),
    ("cargo audit", "reports/cargo-audit.stderr.txt"),
)


class EvidenceError(Exception):
    """Raised when evidence cannot be created or validated."""


def run_git(repo_root: Path, *args: str, env: dict[str, str] | None = None) -> bytes:
    """Run Git and return its stdout bytes."""
    command_env = os.environ.copy()
    if env:
        command_env.update(env)
    result = subprocess.run(
        ["git", "-C", str(repo_root), *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=command_env,
    )
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise EvidenceError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout


def sha256_bytes(content: bytes) -> str:
    """Return the lowercase SHA-256 digest for bytes."""
    return hashlib.sha256(content).hexdigest()


def sha256_file(path: Path) -> str:
    """Return the lowercase SHA-256 digest for a file."""
    digest = hashlib.sha256()
    with path.open("rb") as reader:
        for block in iter(lambda: reader.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path: Path, value: Any) -> None:
    """Write deterministic, human-readable JSON."""
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def load_json(path: Path) -> Any:
    """Load one JSON document with an evidence-oriented error."""
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise EvidenceError(f"cannot load JSON {path}: {error}") from error


def snapshot_tree(repo_root: Path) -> str:
    """Create a Git tree for HEAD plus current changes without altering the real index."""
    file_descriptor, temporary_index = tempfile.mkstemp(prefix="inspequte-index-")
    os.close(file_descriptor)
    os.unlink(temporary_index)
    index_env = {"GIT_INDEX_FILE": temporary_index}
    try:
        run_git(repo_root, "read-tree", "HEAD", env=index_env)
        run_git(repo_root, "add", "-A", "--", ".", env=index_env)
        return run_git(repo_root, "write-tree", env=index_env).decode().strip()
    finally:
        Path(temporary_index).unlink(missing_ok=True)


def parse_name_status(raw: bytes) -> list[dict[str, str | None]]:
    """Parse Git's NUL-delimited name-status output."""
    fields = raw.split(b"\0")
    if fields and not fields[-1]:
        fields.pop()
    changes: list[dict[str, str | None]] = []
    index = 0
    while index < len(fields):
        status_token = fields[index].decode("ascii")
        index += 1
        status_code = status_token[0]
        status_name = STATUS_NAMES.get(status_code)
        if status_name is None:
            raise EvidenceError(f"unsupported Git change status: {status_token}")
        if status_code in {"C", "R"}:
            if index + 1 >= len(fields):
                raise EvidenceError("truncated Git rename/copy status")
            old_path = fields[index].decode("utf-8")
            path = fields[index + 1].decode("utf-8")
            index += 2
        else:
            if index >= len(fields):
                raise EvidenceError("truncated Git change status")
            old_path = None
            path = fields[index].decode("utf-8")
            index += 1
        if "\n" in path or (old_path is not None and "\n" in old_path):
            raise EvidenceError("changed paths containing newlines are unsupported")
        changes.append({"path": path, "oldPath": old_path, "status": status_name})
    return sorted(changes, key=lambda change: str(change["path"]))


def clean_verify_root(verify_root: Path) -> None:
    """Remove only the generated verify-input directory and recreate it."""
    if verify_root.name != "verify-input":
        raise EvidenceError(f"refusing to clean unexpected path: {verify_root}")
    if verify_root.exists():
        shutil.rmtree(verify_root)
    (verify_root / "changes").mkdir(parents=True)
    (verify_root / "reports").mkdir()


def prepare(repo_root: Path, rule_id: str, base_ref: str, attempt: int) -> None:
    """Prepare an isolated review bundle and capture its exact source identity."""
    if RULE_ID_PATTERN.fullmatch(rule_id) is None:
        raise EvidenceError(f"invalid rule ID: {rule_id}")
    if not 1 <= attempt <= 3:
        raise EvidenceError("attempt must be between 1 and 3")

    spec_path = repo_root / "src" / "rules" / rule_id / "spec.md"
    if not spec_path.is_file():
        raise EvidenceError(f"missing rule spec: {spec_path}")

    head_commit = run_git(repo_root, "rev-parse", "HEAD").decode().strip()
    base_commit = (
        run_git(repo_root, "merge-base", base_ref, "HEAD").decode().strip()
        if base_ref
        else head_commit
    )
    tree_sha = snapshot_tree(repo_root)
    diff = run_git(repo_root, "diff", "--binary", base_commit, tree_sha)
    name_status = run_git(repo_root, "diff", "--name-status", "-z", base_commit, tree_sha)
    changes = parse_name_status(name_status)

    verify_root = repo_root / "verify-input"
    clean_verify_root(verify_root)
    shutil.copyfile(spec_path, verify_root / "spec.md")
    (verify_root / "diff.patch").write_bytes(diff)

    current_paths = [
        str(change["path"]) for change in changes if change["status"] != "deleted"
    ]
    deleted_paths = [
        str(change["path"]) for change in changes if change["status"] == "deleted"
    ]
    (verify_root / "changed-files.txt").write_text(
        "".join(f"{path}\n" for path in current_paths), encoding="utf-8"
    )
    (verify_root / "deleted-files.txt").write_text(
        "".join(f"{path}\n" for path in deleted_paths), encoding="utf-8"
    )
    for relative_path in current_paths:
        source = repo_root / relative_path
        if not source.is_file():
            raise EvidenceError(f"changed path is not a regular file: {relative_path}")
        destination = verify_root / "changes" / relative_path
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)

    write_json(
        verify_root / "source.json",
        {
            "schemaVersion": SCRIPT_VERSION,
            "ruleId": rule_id,
            "attempt": attempt,
            "baseRef": base_ref or None,
            "baseCommitSha": base_commit,
            "headCommitSha": head_commit,
            "treeSha": tree_sha,
            "changes": changes,
        },
    )
    (verify_root / "README.md").write_text(
        """# verify-input

`verify-input/` is the only input directory for isolated verification.

- `spec.md`: copied rule contract.
- `diff.patch`: exact patch from the base commit to the reviewed Git tree.
- `source.json`: source identity captured while preparing the bundle.
- `changed-files.txt` and `deleted-files.txt`: sorted changed paths.
- `changes/`: current snapshots of non-deleted files.
- `reports/`: command outputs and `command-status.txt`.
- `manifest.json`: generated after reports and validated before verification.
""",
        encoding="utf-8",
    )
    print(f"Prepared {verify_root} for rule '{rule_id}' at attempt {attempt}.")
    print(f"Source tree: {tree_sha}")


def parse_command_status(path: Path) -> dict[str, int]:
    """Parse the fixed build, test, and audit command status file."""
    expected = {
        "cargo build exit code": "cargo build",
        "cargo test exit code": "cargo test",
        "cargo audit exit code": "cargo audit",
    }
    statuses: dict[str, int] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise EvidenceError(f"cannot read command status {path}: {error}") from error
    for line in lines:
        label, separator, raw_exit_code = line.partition(":")
        command = expected.get(label.strip())
        if not separator or command is None:
            raise EvidenceError(f"invalid command status line: {line}")
        try:
            exit_code = int(raw_exit_code.strip())
        except ValueError as error:
            raise EvidenceError(f"invalid exit code in command status: {line}") from error
        if not 0 <= exit_code <= 255:
            raise EvidenceError(f"exit code out of range in command status: {line}")
        statuses[command] = exit_code
    required_commands = {command for command, _ in REPORT_OUTPUTS}
    missing = sorted(required_commands - set(statuses))
    if missing:
        raise EvidenceError(f"missing command statuses: {', '.join(missing)}")
    return statuses


def command_version(command: list[str]) -> str:
    """Return a compact tool version or an explicit unavailable marker."""
    try:
        result = subprocess.run(
            command,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
    except OSError:
        return "unavailable"
    first_line = next((line.strip() for line in result.stdout.splitlines() if line.strip()), "")
    return first_line or f"exit {result.returncode}"


def changed_file_entries(repo_root: Path, source: dict[str, Any]) -> list[dict[str, Any]]:
    """Build manifest entries for the source snapshot's changed paths."""
    entries = []
    for change in source["changes"]:
        path = str(change["path"])
        status = str(change["status"])
        content_path = repo_root / path
        digest = None if status == "deleted" else sha256_file(content_path)
        entries.append({"path": path, "status": status, "sha256": digest})
    return entries


def validate_relative_path(value: Any, label: str) -> str:
    """Require a repository-relative path that cannot escape its root."""
    if not isinstance(value, str) or not value or "\n" in value:
        raise EvidenceError(f"{label} is invalid")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise EvidenceError(f"{label} must remain inside the repository")
    return value


def validate_source_shape(source: Any) -> dict[str, Any]:
    """Validate the closed source-identity document before using its paths."""
    value = require_keys(
        source,
        {
            "schemaVersion",
            "ruleId",
            "attempt",
            "baseRef",
            "baseCommitSha",
            "headCommitSha",
            "treeSha",
            "changes",
        },
        "source identity",
    )
    if value["schemaVersion"] != SCRIPT_VERSION:
        raise EvidenceError("unsupported source identity schemaVersion")
    if not isinstance(value["ruleId"], str) or RULE_ID_PATTERN.fullmatch(value["ruleId"]) is None:
        raise EvidenceError("source identity ruleId is invalid")
    if not isinstance(value["attempt"], int) or not 1 <= value["attempt"] <= 3:
        raise EvidenceError("source identity attempt is invalid")
    if value["baseRef"] is not None and not isinstance(value["baseRef"], str):
        raise EvidenceError("source identity baseRef is invalid")
    for key in ("baseCommitSha", "headCommitSha", "treeSha"):
        if not isinstance(value[key], str) or GIT_ID_PATTERN.fullmatch(value[key]) is None:
            raise EvidenceError(f"source identity {key} is invalid")
    if not isinstance(value["changes"], list):
        raise EvidenceError("source identity changes must be an array")
    for index, raw_change in enumerate(value["changes"]):
        change = require_keys(
            raw_change, {"path", "oldPath", "status"}, f"source change {index}"
        )
        validate_relative_path(change["path"], f"source change {index} path")
        if change["oldPath"] is not None:
            validate_relative_path(
                change["oldPath"], f"source change {index} oldPath"
            )
        if change["status"] not in STATUS_NAMES.values():
            raise EvidenceError(f"source change {index} status is invalid")
    return value


def create_manifest(repo_root: Path) -> None:
    """Finalize manifest.json after build, test, and audit reports exist."""
    verify_root = repo_root / "verify-input"
    source = validate_source_shape(load_json(verify_root / "source.json"))
    statuses = parse_command_status(verify_root / "reports" / "command-status.txt")
    reports = []
    for command, relative_path in REPORT_OUTPUTS:
        report_path = verify_root / relative_path
        if not report_path.is_file():
            raise EvidenceError(f"missing command report: {report_path}")
        reports.append(
            {
                "command": command,
                "path": relative_path,
                "exitCode": statuses[command],
                "sha256": sha256_file(report_path),
            }
        )

    manifest = {
        "schemaVersion": SCRIPT_VERSION,
        "ruleId": source["ruleId"],
        "attempt": source["attempt"],
        "createdAt": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source": {
            "baseCommitSha": source["baseCommitSha"],
            "headCommitSha": source["headCommitSha"],
            "treeSha": source["treeSha"],
            "diffSha256": sha256_file(verify_root / "diff.patch"),
            "specSha256": sha256_file(verify_root / "spec.md"),
        },
        "changedFiles": changed_file_entries(repo_root, source),
        "reports": reports,
        "tools": {
            "cargo": command_version(["cargo", "--version"]),
            "cargoAudit": command_version(["cargo", "audit", "--version"]),
            "git": command_version(["git", "--version"]),
            "java": command_version(["java", "-version"]),
            "rustc": command_version(["rustc", "--version"]),
        },
    }
    write_json(verify_root / "manifest.json", manifest)
    print(f"Created {verify_root / 'manifest.json'}.")


def require_keys(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    """Require an object with exactly the named keys."""
    if not isinstance(value, dict):
        raise EvidenceError(f"{label} must be an object")
    actual = set(value)
    if actual != keys:
        raise EvidenceError(
            f"{label} keys differ: missing={sorted(keys - actual)}, "
            f"unexpected={sorted(actual - keys)}"
        )
    return value


def validate_manifest_shape(manifest: Any) -> dict[str, Any]:
    """Validate the closed manifest contract before recomputing evidence."""
    value = require_keys(
        manifest,
        {
            "schemaVersion",
            "ruleId",
            "attempt",
            "createdAt",
            "source",
            "changedFiles",
            "reports",
            "tools",
        },
        "manifest",
    )
    if value["schemaVersion"] != SCRIPT_VERSION:
        raise EvidenceError("unsupported manifest schemaVersion")
    if not isinstance(value["ruleId"], str) or RULE_ID_PATTERN.fullmatch(value["ruleId"]) is None:
        raise EvidenceError("manifest ruleId is invalid")
    if not isinstance(value["attempt"], int) or not 1 <= value["attempt"] <= 3:
        raise EvidenceError("manifest attempt is invalid")
    try:
        datetime.fromisoformat(str(value["createdAt"]).replace("Z", "+00:00"))
    except ValueError as error:
        raise EvidenceError("manifest createdAt is invalid") from error
    source = require_keys(
        value["source"],
        {"baseCommitSha", "headCommitSha", "treeSha", "diffSha256", "specSha256"},
        "manifest source",
    )
    for key in ("baseCommitSha", "headCommitSha", "treeSha"):
        if not isinstance(source[key], str) or GIT_ID_PATTERN.fullmatch(source[key]) is None:
            raise EvidenceError(f"manifest source {key} is invalid")
    for key in ("diffSha256", "specSha256"):
        if not isinstance(source[key], str) or SHA256_PATTERN.fullmatch(source[key]) is None:
            raise EvidenceError(f"manifest source {key} is invalid")
    if not isinstance(value["changedFiles"], list):
        raise EvidenceError("manifest changedFiles must be an array")
    if not isinstance(value["reports"], list) or not value["reports"]:
        raise EvidenceError("manifest reports must be a non-empty array")
    if not isinstance(value["tools"], dict) or not value["tools"]:
        raise EvidenceError("manifest tools must be a non-empty object")
    return value


def validate(repo_root: Path) -> None:
    """Reject stale, missing, or mismatched evidence deterministically."""
    verify_root = repo_root / "verify-input"
    manifest = validate_manifest_shape(load_json(verify_root / "manifest.json"))
    source = validate_source_shape(load_json(verify_root / "source.json"))
    for key in ("ruleId", "attempt", "baseCommitSha", "headCommitSha", "treeSha"):
        manifest_value = manifest["source"].get(key, manifest.get(key))
        if manifest_value != source[key]:
            raise EvidenceError(f"manifest does not match source identity for {key}")

    current_head = run_git(repo_root, "rev-parse", "HEAD").decode().strip()
    current_tree = snapshot_tree(repo_root)
    if current_head != source["headCommitSha"]:
        raise EvidenceError("stale evidence: HEAD changed after evidence preparation")
    if current_tree != source["treeSha"]:
        raise EvidenceError("stale evidence: source tree changed after evidence preparation")

    expected_diff = run_git(
        repo_root, "diff", "--binary", source["baseCommitSha"], source["treeSha"]
    )
    actual_diff = (verify_root / "diff.patch").read_bytes()
    if actual_diff != expected_diff:
        raise EvidenceError("diff.patch does not match the recorded source tree")
    if sha256_bytes(actual_diff) != manifest["source"]["diffSha256"]:
        raise EvidenceError("diff.patch hash does not match manifest")
    if sha256_file(verify_root / "spec.md") != manifest["source"]["specSha256"]:
        raise EvidenceError("spec.md hash does not match manifest")

    expected_changed_files = changed_file_entries(repo_root, source)
    if manifest["changedFiles"] != expected_changed_files:
        raise EvidenceError("changed file hashes do not match manifest")
    for entry in expected_changed_files:
        if entry["status"] == "deleted":
            continue
        copied_path = verify_root / "changes" / entry["path"]
        if not copied_path.is_file() or sha256_file(copied_path) != entry["sha256"]:
            raise EvidenceError(f"changed file snapshot mismatch: {entry['path']}")

    statuses = parse_command_status(verify_root / "reports" / "command-status.txt")
    expected_reports = []
    for command, relative_path in REPORT_OUTPUTS:
        report_path = verify_root / relative_path
        expected_reports.append(
            {
                "command": command,
                "path": relative_path,
                "exitCode": statuses[command],
                "sha256": sha256_file(report_path),
            }
        )
    if manifest["reports"] != expected_reports:
        raise EvidenceError("command report evidence does not match manifest")
    print(f"Validated {verify_root / 'manifest.json'} against the current source state.")


def repository_root(explicit_root: str | None) -> Path:
    """Resolve the repository root used by the command."""
    if explicit_root:
        return Path(explicit_root).resolve()
    return Path(__file__).resolve().parent.parent


def main() -> int:
    """Run the requested evidence command."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", help=argparse.SUPPRESS)
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("rule_id")
    prepare_parser.add_argument("base_ref", nargs="?", default="")
    prepare_parser.add_argument(
        "--attempt",
        type=int,
        default=int(os.environ.get("RULE_AUTHORING_ATTEMPT", "1")),
    )
    subparsers.add_parser("create-manifest")
    subparsers.add_parser("validate")
    args = parser.parse_args()
    repo_root = repository_root(args.repo_root)

    try:
        if args.command == "prepare":
            prepare(repo_root, args.rule_id, args.base_ref, args.attempt)
        elif args.command == "create-manifest":
            create_manifest(repo_root)
        elif args.command == "validate":
            validate(repo_root)
    except EvidenceError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
