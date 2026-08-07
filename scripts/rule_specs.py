#!/usr/bin/env python3
"""Generate rule documentation from OKF rule specifications."""

from __future__ import annotations

import argparse
import ast
import shutil
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parent.parent
RULES_ROOT = REPO_ROOT / "src" / "rules"
DOCS_ROOT = REPO_ROOT / "docs" / "rules"


class RuleSpecError(Exception):
    """Raised when a rule specification cannot be processed."""


def parse_frontmatter(spec_path: Path) -> tuple[dict[str, Any], str]:
    """Parse the flat YAML subset used by inspequte rule specifications."""
    text = spec_path.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)
    if not lines or lines[0].rstrip("\r\n") != "---":
        raise RuleSpecError(f"missing OKF frontmatter: {spec_path}")

    try:
        closing_index = next(
            index
            for index, line in enumerate(lines[1:], start=1)
            if line.rstrip("\r\n") == "---"
        )
    except StopIteration as error:
        raise RuleSpecError(f"unterminated OKF frontmatter: {spec_path}") from error

    metadata: dict[str, Any] = {}
    for line_number, line in enumerate(lines[1:closing_index], start=2):
        stripped = line.strip()
        if not stripped:
            continue
        key, separator, raw_value = stripped.partition(":")
        if not separator or not key or not raw_value.strip():
            raise RuleSpecError(
                f"invalid frontmatter entry at {spec_path}:{line_number}"
            )
        if key in metadata:
            raise RuleSpecError(
                f"duplicate frontmatter key {key!r} at {spec_path}:{line_number}"
            )
        try:
            metadata[key] = ast.literal_eval(raw_value.strip())
        except (SyntaxError, ValueError) as error:
            raise RuleSpecError(
                f"frontmatter values must use quoted strings or inline lists at "
                f"{spec_path}:{line_number}"
            ) from error

    body = "".join(lines[closing_index + 1 :])
    return metadata, body


def require_string(metadata: dict[str, Any], key: str, spec_path: Path) -> str:
    """Return a required, non-empty string metadata field."""
    value = metadata.get(key)
    if not isinstance(value, str) or not value.strip():
        raise RuleSpecError(f"missing non-empty {key!r} in {spec_path}")
    return value


def generate_docs() -> None:
    """Copy rule concepts and generate an OKF v0.2 bundle index."""
    if not RULES_ROOT.is_dir():
        raise RuleSpecError(f"rules directory does not exist: {RULES_ROOT}")

    rule_specs: list[tuple[str, Path, dict[str, Any]]] = []
    seen_rule_ids: set[str] = set()
    for rule_dir in sorted(path for path in RULES_ROOT.iterdir() if path.is_dir()):
        if not any(rule_dir.iterdir()):
            continue
        spec_path = rule_dir / "spec.md"
        if not spec_path.is_file():
            raise RuleSpecError(f"missing spec.md: {spec_path}")

        metadata, _ = parse_frontmatter(spec_path)
        rule_id = require_string(metadata, "rule_id", spec_path)
        normalized_rule_id = rule_id.casefold()
        if normalized_rule_id in seen_rule_ids:
            raise RuleSpecError(f"duplicate rule ID detected: {rule_id}")
        seen_rule_ids.add(normalized_rule_id)
        rule_specs.append((rule_dir.name, spec_path, metadata))

    if not rule_specs:
        raise RuleSpecError(f"no rule directories found under {RULES_ROOT}")

    DOCS_ROOT.mkdir(parents=True, exist_ok=True)
    for output_path in DOCS_ROOT.glob("*.md"):
        if output_path.name != "index.md":
            output_path.unlink()

    statuses: dict[str, list[tuple[str, dict[str, Any]]]] = {}
    for path_id, spec_path, metadata in rule_specs:
        shutil.copyfile(spec_path, DOCS_ROOT / f"{path_id}.md")
        status = require_string(metadata, "status", spec_path)
        statuses.setdefault(status, []).append((path_id, metadata))

    index_lines = ["---\n", 'okf_version: "0.2"\n', "---\n", "\n", "# Rules\n"]
    for status in sorted(statuses):
        index_lines.extend(("\n", f"## {status.title()}\n", "\n"))
        for path_id, metadata in statuses[status]:
            title = require_string(metadata, "title", RULES_ROOT / path_id / "spec.md")
            description = require_string(
                metadata, "description", RULES_ROOT / path_id / "spec.md"
            )
            rule_id = require_string(
                metadata, "rule_id", RULES_ROOT / path_id / "spec.md"
            )
            index_lines.append(
                f"- [{title}](./{path_id}.md) - {description} (`{rule_id}`)\n"
            )

    (DOCS_ROOT / "index.md").write_text("".join(index_lines), encoding="utf-8")
    print(f"Generated docs in {DOCS_ROOT} (rules: {len(rule_specs)}).")


def main() -> int:
    """Run the selected rule-spec command."""
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("generate",))
    args = parser.parse_args()

    try:
        if args.command == "generate":
            generate_docs()
    except RuleSpecError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
