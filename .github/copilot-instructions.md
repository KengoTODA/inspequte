# Review guidance

Read `AGENTS.md` and, for rule changes, `src/rules/AGENTS.md`.
Prioritize bytecode correctness, false positives/negatives, deterministic SARIF,
and compatibility with shared analysis helpers.
For formal Go/No-Go review, use `.codex/skills/inspequte-rule-verify/SKILL.md`.
Formal review requires validated evidence; ordinary PR review can inspect relevant
repository code and must not claim a formal verification result without evidence.
