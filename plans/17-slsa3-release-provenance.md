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
validation and verifier failure propagation without real publishing. Use Gradle
local publication and Portal `--validate-only` to check the artifact task graph,
and reject tampered files with a stub verifier. Validate source formatting.
Actual OIDC attestations and publication require GitHub-hosted release jobs.

## Dependencies and remaining operational checks

GitHub artifact attestations, hosted runners, GitHub CLI, existing release
environment/Portal credentials and crates.io trusted publishing configuration.
Protect builder workflow changes with repository rules/review and review the
first real release's certificate/source identities before advertising L3.
Extending provenance to the exact crates.io package needs a separate publication
design; `cargo publish` repackages source. Consumer integrations can adopt the
documented verification policy separately.

## References

- [GitHub article](https://github.blog/enterprise-software/devsecops/enhance-build-security-and-reach-slsa-level-3-with-github-artifact-attestations/)
- [GitHub L3 guide](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating)
- [release-please-action](https://github.com/googleapis/release-please-action)
