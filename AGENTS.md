# Agent guidance for this repository

These rules apply to every agent that works in this repository, Claude and
Codex alike. Read this file before you start a task.

## Project

- This project is a Rust multi-user terminal multiplexer for coding agents.
  It is modeled on herdr and luvus.
- Research is complete and lives under docs/research/. Read the relevant
  document before you design or build.
- The first build task is the multiplayer MVP: one server on Linux, clients
  on Linux and macOS, one workspace tree per user, static token auth,
  read-only cross-user viewing. Test cases T1 to T9 define done.

## Settled decisions

Do not reopen these. The reasons are in docs/research/.

- PTYs and shells run on the server, never on the client.
- Process model: one broker plus one runtime per user. The broker owns
  identity, routing, supervision, and metadata. It never owns PTYs or agent
  child processes.
- TUI stack: ratatui plus crossterm.
- Linux and macOS only. No Windows.
- GPUI desktop app is far future. Keep core types frontend-neutral, nothing
  more.
- Reference clones at /home/nethum/Projects/_research/herdr and
  /home/nethum/Projects/_research/luvus are read only. Never modify them.

## Quality gates

These gates apply to all Rust code in this repository. CI will enforce them
once the workspace exists. Until then, check them yourself before every PR.

- Cognitive complexity per function: less than 22 (clippy).
- Cyclomatic complexity per function: less than 22.
- Lines per file: less than 500.
- Test coverage: 100 percent of changed lines (cargo llvm-cov).
- Mutation testing: 0 surviving mutants in changed code (cargo-mutants).
- Dead code: 0. No unused functions, no unused dependencies (cargo-udeps).
  Do not hide dead code with allow attributes.
- Redundant code: 0. Search for an existing helper before you write a new
  one. Do not duplicate logic.
- Escape hatches: no unwrap or expect outside tests. Every allow attribute
  needs a one-line reason. Every unsafe block needs a safety comment.
- cargo clippy with warnings denied must pass. cargo fmt must pass.

## Git

- Never push to main. Create a branch, push with git push -u origin, then
  open a PR.
- Run tests and lint before every push.
- Add a DCO Signed-off-by line to every commit.
- Never add co-author attribution of any kind.
- Review the staged file list before committing. Drop unrelated files and
  scratch artifacts.

## Writing style

- Use ASD-STE100 Simplified Technical English in comments, docs, commits,
  and PR text.
- Plain ASCII only. No emojis, no em dashes, no decorative formatting.
- Be simple, brief, and direct. Write for a non-native English speaker.
