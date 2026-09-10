# ADR 0003: Build macOS binaries on a GitHub Actions runner

Date: 2026-09-04
Status: Accepted

## Context

The owner builds and publishes releases from one Linux PC. Before
iroh, macOS assets were cross built there with cargo-zigbuild. iroh
ends that.

iroh pulls in Apple frameworks through many crates. The failed link
of aarch64-apple-darwin in PR 175 shows Security, CoreFoundation,
SystemConfiguration, Foundation, and libobjc on the linker line,
brought in by netwatch, netdev, portmapper, reqwest, and
rustls-platform-verifier. The link stops with "unable to find
dynamic system library 'objc'". Zig has no Apple SDK, so it cannot
supply these.

Issue 173 tried a smaller fix first: a patch fork of iroh-dns and
iroh-relay that drops the hickory-resolver system-config feature on
macOS. The fork branch seer/no-system-config-macos on
nethum529/iroh was built and used through [patch.crates-io] in
PR 175. It did not work. hickory-resolver was only the first
blocker; the frameworks above remain. PR 175 was closed and the
fork branch stays only as a record.

Issue 172 gave the friend kyler505 two paths: Path A, a GitHub
Actions macOS runner in the public repo nethum529/seer-releases, or
Path B, a build by hand on their own Mac. kyler505 chose Path A
because it is the repeatable path and it is free for a public repo.
Issue 179 built it.

## Decision

macOS binaries are built on a GitHub Actions macos runner in the
public repo nethum529/seer-releases. They are not cross built from
Linux.

### Workflow

- The workflow source lives in this repository at
  scripts/seer-releases/macos-build.yml. scripts/release.sh copies
  it into the seer-releases checkout at
  .github/workflows/macos-build.yml, the same way it copies
  install.sh. One source of truth, one place to edit.
- The trigger is workflow_dispatch with two inputs: tag, the
  release tag, and ref, the source commit to build. The workflow
  never creates a release. The release must exist first.
- The job runs on macos-latest with permissions contents: write.
- Step 1 checks out the private source repository with
  actions/checkout@v4 and ssh-key: secrets.SEER_SOURCE_DEPLOY_KEY.
  The secret holds a read only deploy key. The owner creates the
  key and sets the secret. The workflow file names the source
  repository nethum529/seer.
- Step 2 adds the x86_64-apple-darwin target. The runner is arm64,
  so aarch64-apple-darwin is already present.
- Steps 3 and 4 run cargo build --release --package seer for both
  darwin targets.
- Step 5 signs each of the six binaries with an ad hoc signature:
  codesign --force --sign - for seer, seer-broker, and seer-runtime
  in both target directories. The linker signature alone is not
  enough. macOS taskgated kills a binary that carries only that
  signature (issue 362).
- Step 6 runs the seer-runtime of the runner architecture and
  checks that it reaches argument parsing. The step fails when
  macOS kills the runtime before it starts.
- Step 7 makes seer-darwin-arm64.tar.gz and
  seer-darwin-x86_64.tar.gz from the three binaries seer,
  seer-broker, and seer-runtime. Same asset names and layout as the
  Linux asset.
- Step 8 uploads both archives with gh release upload --clobber to
  the tag, using github.token.

### release.sh

- Before any build, release.sh checks that
  SEER_SOURCE_DEPLOY_KEY is listed by gh secret list. If it is not,
  it prints the fix and exits 1.
- release.sh builds and publishes the Linux asset first with
  gh release create.
- After that it runs gh workflow run macos-build.yml with
  tag and ref set to the current HEAD, reads the newest run id with
  gh run list, and waits with gh run watch --exit-status. On
  failure it prints "macOS build failed, see the run in
  seer-releases" and exits 1. The release then holds only the Linux
  asset and the owner reruns the workflow by hand.
- --dry-run prints every new command instead of running it, and
  skips the secret check.

## Consequences

- Every macOS release signs its binaries with an ad hoc signature
  before it packages them. A release without this step ships a
  seer-runtime that macOS kills, which the client then reports as a
  missing socket (issue 362).
- A release needs the network and a working GitHub Actions runner.
  The Linux part still works offline; the macOS part does not.
- The private source repository is exposed to seer-releases through
  one read only deploy key. The key is the only shared secret.
- Two repositories must stay in step. release.sh pushes the
  workflow file and install.sh to seer-releases on every release
  that changes them, with commit message "Update release files".
- macOS assets appear on a release later than the Linux asset. A
  user who fetches the release between the two steps sees only the
  Linux asset.
- The prerelease v0.2.0-test1 proved the path end to end: the
  workflow checked out the private repo with the secret, and
  kyler505 installed and ran the arm64 asset on macOS 26.6.2
  (issue 172).
- The iroh patch fork is dead. Do not try a Linux cross build for
  darwin again without an Apple SDK.
