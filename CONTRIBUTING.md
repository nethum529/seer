# Contributing to Seer

Seer accepts work from people who write code directly and from people who use
coding agents. The contributor owns the change and its review.

## Organize the work

- Use one GitHub issue for each change.
- Write each ticket with Given, When, and Then acceptance criteria.
- Anyone can file a ticket.
- Before you start, read the issue and its comments.
- Claim an unclaimed ticket with a comment on the issue.
- Keep one pull request for one ticket.
- Do not add work that the ticket does not request.

Useful commands:

```sh
gh issue view N
gh issue comment N --body "I will work on this ticket."
```

## Create the branch

Fetch current work before you create the branch:

```sh
git fetch origin
git switch -c <branch> origin/main
```

If the ticket names a current feature branch as its base, branch from that
remote branch instead:

```sh
git switch -c <branch> origin/<feature-branch>
```

Never push to main. Keep all follow-up fixes as new commits on the same feature
branch.

## Follow the repository rules

Read AGENTS.md and CLAUDE.md before you change a file. The rules in those files
apply to contributors and coding agents.

All Rust code must meet these limits:

- Cognitive complexity is less than 22 for each function.
- Cyclomatic complexity is less than 22 for each function.
- Each file has fewer than 500 lines.
- Dead code is zero. Do not keep unused functions or dependencies.
- Do not hide dead code with an allow attribute.
- Redundant code is zero. Search for an existing helper before you add one.
- Do not use unwrap or expect outside tests.
- Each allow attribute has a one-line reason.
- Each unsafe block has a safety comment.

Run these gates before each push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
cargo udeps --workspace --all-targets
```

Install cargo-udeps before you run the last command if it is not installed.

Tests are a cost. The default is no new test. Add a test only for a user-facing
contract or for a bug that came back. Write the test before the code. Do not add
a test for coverage or for a private implementation detail. Do not duplicate a
test. Keep test helpers small. Do not build a test framework. When you change a
module, remove tests that only pin internals, duplicate another test, or exist
only for coverage.

Comments are also a cost. Keep only a safety comment, the reason for an allow
attribute, or a fact that the code cannot show. Do not restate the code. Do not
add a doc comment to a private item. Add a doc comment to a public item only
when its name is not sufficient.

## Commit and open the pull request

Review the changed and staged file lists. Remove unrelated files and scratch
artifacts. Sign every commit for the Developer Certificate of Origin:

```sh
git status --short
git diff --staged --name-only
git commit -s -m "Short change summary"
```

Do not add a co-author line. Push the feature branch:

```sh
git push -u origin <branch>
```

Use the issue title as the pull request title. Start the pull request body with
`Refs #N`:

```sh
gh pr create --base main --title "<issue title>" --body "Refs #N

<short summary and gate results>"
```

## Respond to review

The maintainer reviews one exact head commit. The review comment names that
commit and gives numbered blockers.

- Fix every numbered blocker.
- Commit each follow-up fix on the same branch.
- Push the same branch. Do not open a second pull request.
- Read the next review against the new head commit.

## Run a manual check

On Linux, build all three binaries, start the server, and attach the owner
client:

```sh
cargo build --workspace
target/debug/seer start
target/debug/seer
```

Press Control-Q to leave the TUI. See [docs/development.md](docs/development.md)
for an isolated two-person check on one machine.

## macOS notes

- The workspace builds on macOS.
- The client runs on macOS. The broker, runtimes, PTYs, and shells run on Linux.
- `seer start` runs on Linux only.
- Port 7321 must be free for the `seer start` tests.
- Unix socket paths must stay short. The runtime rejects a path over 99 bytes.
  Use short temporary paths and a short XDG_RUNTIME_DIR value.

See [docs/development.md](docs/development.md) for full setup and check steps.
