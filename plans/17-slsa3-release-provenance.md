# SLSA Build Level 3 release provenance

## Objective and background

Adopt GitHub's reusable-builder pattern for the CLI and Gradle plugin, with
verification before publication and instructions for consumers. Complexity: High.
Attesting a binary before packaging does not attest the archive users download.
An arbitrary caller-supplied artifact or digest is not a trusted build input.

## Implementation and TODOs

1. Define the protected artifacts: the five CLI release archives and the Gradle
   plugin implementation, sources and javadoc JARs plus generated publication
   metadata. Exclude crates.io source packages, GitHub-generated source archives,
   benchmark attachments and downstream rebuilds from the L3 claim.
2. Move fixed build/packaging steps into component-specific reusable workflows.
   Build from the triggering commit; validate release-please's SHA and tag against
   it so the checkout agrees with the platform-generated provenance identity.
3. Attest final archives and Gradle publication files in fresh hosted jobs within
   those workflows. Transfer only outputs from their own build jobs.
4. Give builders read access, attesters OIDC/attestation writes, and publishers
   only their required publication credentials. Do not inherit secrets.
5. Verify bytes, repository, reusable signer, workflow digest, source digest and
   hosted-runner identity before publication. Prevent Gradle archive/metadata
   regeneration. Verify uploaded draft CLI assets before making the release
   public. Document consumer verification.

## Release-please constraints and decisions

- Keep release orchestration in the same workflow run using component-specific
  `release_created`, `tag_name` and `sha` outputs. A reusable workflow is a job,
  not a step, and cannot be dispatched dynamically by a release-please step.
- GITHUB_TOKEN-created releases/tags do not trigger another workflow. The current
  GitHub App token can trigger events, but publication does not depend on them.
- Local reusable workflow references use the caller commit. Fail closed if a
  delayed release resolves to another SHA; do not silently attribute that build
  to the current caller commit. Recovery requires a run at the release commit.
- Both components use draft GitHub releases; publish drafts only after all
  mandatory build/attestation/verification jobs succeed. Portal publication is
  separately gated and uses the verified files without rebuilding them.

## Tests and success criteria

Validate workflow syntax and permissions with actionlint. Exercise source/tag
validation and verifier failure propagation without real publishing. Prepare
Gradle publication files, restore them on a simulated publishing runner and
exercise the actual pre-upload verification hook with a digest-checking stub.
Remove the Portal task action in the test init script to prevent remote access.
Validate source formatting.
Actual OIDC attestations and publication require GitHub-hosted release jobs.

## Implementation evidence and lessons

Implemented in [PR #401](https://github.com/KengoTODA/inspequte/pull/401), with
separate commits for scope, component builders, publication gates and tests/docs.
[CI run 38139486530](https://github.com/KengoTODA/inspequte/actions/runs/38139486530)
passed the CLI, Gradle plugin, Java 26 and release-provenance jobs on commit
`33eaf9055b3650c737cf223ed39d9691b6e9412c`.
The release job used Java 21.0.10 and Gradle 8.14.4, prepared the actual publication
files successfully, and passed 10 shell-policy tests plus the restored-file and
tampering integration test (all exit codes 0). The Docs build passed strict mode.
Local actionlint 1.7.12, shell syntax, cargo fmt and git diff checks also passed.

- Prepare only publication-attached archives: Kotlin's additional sources task
  shares an output path with the Java sources archive and must not overwrite it.
- Plugin Publish 2.1.1 requires real credentials even with `--validate-only`.
  PR tests therefore retain the build's pre-upload action and remove the remote
  Portal action; real credentials stay confined to the release environment.
- Local Gradle wrapper downloads timed out. CI with the pinned wrapper owns the
  Gradle validation evidence; no local Gradle success is claimed.

## Dependencies and remaining operational checks

GitHub artifact attestations, hosted runners, GitHub CLI, existing release
environment/Portal credentials and crates.io trusted publishing configuration.
Protect builder workflow changes with repository rules/review and review the
first real release's certificate/source identities before advertising L3.
Extending provenance to the exact crates.io package needs a separate publication
design; `cargo publish` repackages source. Consumer integrations can adopt the
documented verification policy separately.

## Follow-up review against the 2026-10-11 article

Reviewed the caller/build/attestation/publication flow against Songmu's article
and the pinned action's source. Replaced the older `attest-build-provenance`
action with `actions/attest` v4 at
`1e69f48acb82d1966a394da916b4c1698aa569d6`. Default provenance mode and explicit
subject paths preserve the protected artifact scope. Disable OCI storage records
and retain the existing file-attestation permissions.

The repository's `GET /repos/KengoTODA/inspequte/immutable-releases` response on
2026-10-11 was `{"enabled":true,"enforced_by_owner":false}`. No setting change
was necessary. All CLI asset uploads, including benchmarks, precede publication;
the draft-first flow is compatible with immutable releases.

The exact source/signer digest and reusable workflow verification policy agrees
with the article. Gradle's official publication documentation confirms that
modern Plugin Publish uses Maven publications for JARs and metadata. Verification
of the actual bytes served by the Portal remains a first-release check.

Documented reproducibility gaps separately: floating Rust/runner versions,
archive metadata, and no independent rebuild/hash comparison. Bit-for-bit
reproducibility is not established by the current attestation tests and remains
a follow-up rather than a claimed SLSA Build L3 property.

## References

- [GitHub article](https://github.blog/enterprise-software/devsecops/enhance-build-security-and-reach-slsa-level-3-with-github-artifact-attestations/)
- [GitHub L3 guide](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating)
- [release-please-action](https://github.com/googleapis/release-please-action)
- [Songmu's artifact attestation article](https://songmu.jp/riji/entry/2026-10-11-github-actions-artifact-attestations.html)
- [Consolidated attestation action](https://github.com/actions/attest/tree/1e69f48acb82d1966a394da916b4c1698aa569d6)
- [Gradle Plugin Portal publication](https://plugins.gradle.org/docs/publish-plugin)
