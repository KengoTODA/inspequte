---
name: inspequte-cargo-audit-fix
description: Fix Rust dependency audit failures in inspequte by reading cargo audit SARIF output, updating Cargo.toml or Cargo.lock with minimal dependency changes, and verifying with cargo build, cargo test, and cargo audit.
---

# inspequte cargo audit fix

## Inputs
- `cargo audit --format sarif` output, usually `target/audit-input/audit.sarif` or `target/audit.sarif`.
- `Cargo.toml` and `Cargo.lock`.
- The minimum surrounding files needed to understand dependency use.

## Outputs
- Minimal dependency updates in `Cargo.lock`.
- `Cargo.toml` updates only when a direct dependency constraint prevents resolving a fixed version.
- A clear final report listing fixed advisories and any unresolved manual follow-ups.

## Workflow
1. Read the SARIF report and identify each affected crate, current version, advisory ID, severity, and fixed version or patched range.
2. Prefer lockfile-only updates:
   - Run `cargo update -p <crate> --precise <fixed_version>` when SARIF or RustSec data gives one concrete fixed version.
   - If only a version range is available, choose the lowest compatible patched version.
3. Edit `Cargo.toml` only when the fixed version cannot be selected under the existing direct dependency requirement.
4. Keep changes narrowly scoped to the vulnerable dependency and resolver-required transitive updates.
5. Do not add compatibility shims, alternate dependency paths, or unrelated crate upgrades.
6. Run `cargo fmt` after code or manifest changes.
7. Verify with:
   - `cargo build`
   - `cargo test`
   - `cargo audit --format sarif`
8. If verification fails, continue only when the next step is clear from the error. Otherwise stop and report:
   - unresolved crate or advisory
   - attempted update
   - blocking error
   - required human decision

## Guardrails
- Treat the SARIF report as the source of truth for the audit failure being fixed.
- Prefer simplicity over backward compatibility.
- Do not update unrelated dependencies for freshness.
- Do not ignore advisories or add audit suppressions.
- Preserve Conventional Commit style when committing: `fix(deps): resolve cargo audit findings`.
- Use ASCII-only edits unless a touched file already requires Unicode.

## Definition of Done
- `cargo audit --format sarif` exits successfully.
- `cargo build` and `cargo test` pass.
- The diff is limited to dependency files unless source changes are necessary for the dependency update.
- Any unresolved advisory is documented with concrete evidence.
