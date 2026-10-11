# Verifying release provenance

Releases built after this change use the GitHub reusable-builder pattern for
[SLSA v1 Build Level 3](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/increase-security-rating).
Earlier releases have no attestations from these workflows. Successful execution
of the first release and review of its provenance are required before claiming L3
for that release. An attestation establishes build origin, not freedom from bugs.

## Protected artifacts

| Component | Attested files | Reusable signer |
| --- | --- | --- |
| CLI | Final `inspequte-<tag>-<target>.tar.gz` / `.zip` archives, including the executable, README and license | `.github/workflows/build-cli-release.yml` |
| Gradle plugin | Implementation, sources and javadoc JARs; generated implementation/marker POMs and any generated module metadata | `.github/workflows/build-gradle-release.yml` |

The CLI targets are `amd64-unknown-linux-gnu`, `arm64-unknown-linux-gnu`,
`arm64-apple-darwin`, `amd64-apple-darwin` and `amd64-pc-windows-msvc`.
The last target uses ZIP; the others use tar.gz. Verify the downloaded archive
before extraction, and the plugin files before loading them into Gradle/JVM.
Attestations are available through GitHub's attestation API and GitHub CLI.

The scope excludes the crates.io `.crate` source package, GitHub-generated source
archives, benchmark attachments, repository source checkouts, and downstream
builds (including `cargo install` and Homebrew rebuilds). Those are different
artifacts and do not acquire L3 provenance from these release files.

## Consumer verification

Authenticate the GitHub CLI and obtain the expected release commit through a
trusted release record. The examples resolve an existing tag through GitHub;
repository/tag protection and review of builder changes remain part of that
trust decision. Use the commit recorded in your dependency policy when available.
Do not infer the expected identity from an unverified attestation.

For a CLI release, replace the tag and target with the desired release:

```bash
repository=KengoTODA/inspequte
tag=inspequte-v1.2.3  # Example only; choose a release with the new attestations.
asset="inspequte-${tag}-amd64-unknown-linux-gnu.tar.gz"
release_sha="$(gh api "repos/$repository/commits/$tag" --jq '.sha')"
gh release download "$tag" --repo "$repository" --pattern "$asset"
gh attestation verify "$asset" --repo "$repository" \
  --signer-workflow "$repository/.github/workflows/build-cli-release.yml" \
  --signer-digest "$release_sha" --source-digest "$release_sha" \
  --deny-self-hosted-runners
# Extract/install only after verification succeeds.
```

For Gradle, download files from the Plugin Portal's Maven repository without
executing the plugin. Use the same policy on each JAR and POM/module metadata:

```bash
repository=KengoTODA/inspequte
version=0.6.2  # Example only; choose a release with the new attestations.
tag="gradle-plugin-v${version}"
release_sha="$(gh api "repos/$repository/commits/$tag" --jq '.sha')"
base="https://plugins.gradle.org/m2/io/github/kengotoda/inspequte/inspequte-gradle-plugin/$version"
artifact="inspequte-gradle-plugin-${version}.jar"
curl --fail --location --output "$artifact" "$base/$artifact"
gh attestation verify "$artifact" --repo "$repository" \
  --signer-workflow "$repository/.github/workflows/build-gradle-release.yml" \
  --signer-digest "$release_sha" --source-digest "$release_sha" \
  --deny-self-hosted-runners
```

The Portal's filename can differ from the local attestation subject name
(`pom-default.xml` / `module.json`). Verification checks the actual content digest.
Repeat for sources/javadoc JARs and the implementation/marker POMs; do not treat
one verified implementation JAR as verification of the whole dependency graph.
Gradle dependency verification can retain approved hashes for subsequent builds,
but its hashes alone do not check SLSA provenance. Integrating this policy into
`setup-inspequte` or automatic Gradle downloads is separate consumer work.

## Build and publication policy

Release-please creates component-specific draft releases and tags, then calls
each reusable builder as a downstream job in the same run. Each builder has
fixed build steps and targets and accepts only a release tag and source SHA.
Both must resolve to the triggering commit; this keeps GitHub's source identity
consistent with the checkout. Release builders do not restore build caches.

Build jobs have `contents: read` and no publication secrets or OIDC signing
permission. Fresh hosted attestation jobs consume outputs from their own builder
and have `id-token: write`, `attestations: write` and `contents: read`. They do not
checkout or execute project code. CLI attesters download artifacts by exact
target name, so callers cannot supply additional artifacts through a wildcard.
All actions are pinned to full commit SHAs.
The attesters use the consolidated `actions/attest` action in its default SLSA
provenance mode. OCI storage records are disabled; file attestations do not
require `artifact-metadata: write`.

Publishers have no attestation-writing or OIDC permission (the separate crates.io
trusted publisher retains its required OIDC permission). CLI publication verifies
the full five-archive inventory, uploads it to the draft, downloads it again and
verifies the served bytes before making the release public. Any failed check
stops publication. Gradle publication restores the prepared files and disables
archive/POM/module metadata generation. Immediately before the Portal upload,
it verifies the actual publication files, including generated metadata. Plugin
Portal approval/availability can lag behind upload success.

Verification requires the repository, reusable workflow path, exact signer
workflow commit, exact source commit, and GitHub-hosted runner identity. The local
reusable workflow and source use the same triggering commit; provenance can have
`refs/heads/main` as its source ref even when an artifact is named after a release
tag. The verification policy deliberately checks the exact commit digest.

## Release immutability and reproducibility

Keep [GitHub release immutability](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes)
enabled. The repository API reported `enabled: true` on 2026-10-11. This setting
applies to future releases and locks their tags and assets at publication.
Upload every asset, including benchmark attachments, while the release is a
draft. The CLI publication job waits for the benchmark upload, then verifies
the archives before making the draft public. Do not plan asset replacement
after publication; publish a new release when corrections are needed.

[Songmu's supply-chain article](https://songmu.jp/riji/entry/2026-10-11-github-actions-artifact-attestations.html)
also recommends reducing variation in tool versions and archive metadata.
This pipeline does not yet guarantee byte-for-byte reproducible rebuilds:
Rust uses the floating `stable` channel, some hosted runner labels use `latest`,
and tar/ZIP packaging does not normalize timestamps, ownership or permissions.
Pinning an action's implementation SHA does not pin the toolchain or runner image
it installs. Java and Gradle versions are fixed, but identical plugin output from
two independent clean builds has not been tested.

Follow-up work should pin release toolchains, control the build environment,
normalize archive metadata where supported, and compare hashes from two clean
builds for every target and plugin publication file. Successful attestation
verification proves the origin and integrity of the downloaded bytes, not that
another build will reproduce them. The [SLSA Build L3 requirements](https://slsa.dev/spec/v1.2/build-track-basics#build-l3-hardened-builds)
focus on isolation and protecting provenance signing from build steps;
reproducibility is a separate property.

## Release-please limitations and operator TODOs

- Reusable workflows must be job-level `uses`, with permissions on both caller
  and callee. Keep the `gradle-plugin--...` output prefix for the monorepo
  component. No `secrets: inherit` is required by either builder.
- `GITHUB_TOKEN`-created tags/releases do not start additional workflows. The
  existing App token avoids that limitation, but this pipeline uses downstream
  jobs and does not depend on tag/release events.
- If a delayed release points to an older commit, the source guard fails rather
  than misattributes provenance. Recovery must run the orchestration at that
  release commit with its builder definitions and tag. A manual recovery entry
  point is a separate follow-up; rerunning a release-please job may no longer
  return `release_created=true`, so do not rely on it to retry publication.
- Protect `main`, release tags, and reusable builder/policy changes through
  repository rules and required reviews. Review workflow updates as changes to
  the trusted build platform. These repository settings are operational TODOs.
- Inspect the first CLI and plugin release's provenance and verify files fetched
  from both distribution services using the policy above. Release-only OIDC
  signing and real publication cannot be exercised by a pull-request test.
- Extend this design to crates.io only after ensuring attestation covers the
  exact published `.crate`; `cargo publish` packages source again. Benchmark and
  source-archive provenance also need their own policy if brought into scope.

See the [implementation plan](https://github.com/KengoTODA/inspequte/blob/main/plans/17-slsa3-release-provenance.md) and the
[GitHub CLI verification reference](https://cli.github.com/manual/gh_attestation_verify).
