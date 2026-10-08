# Agent guidance for this repository

These rules apply to every agent that works in this repository, Claude and
Codex alike. Read this file before you start a task.

## Project

- This project is a Rust multi-user terminal multiplexer for coding agents.
  Seer is the multiplayer and collaboration layer for coding agent terminals:
  one person hosts the room, friends join with one pasted line, and each
  person runs their own tree of shells on their own computer. herdr and luvus stay the reference
  code bases for the terminal core.
- Research is complete and lives under docs/research/. Read the relevant
  document before you design or build.
- The first build task is the multiplayer MVP: one server on Linux, clients
  on Linux and macOS, one workspace tree per user, static token auth,
  read-only cross-user viewing. Test cases T1 to T9 define done.

## Settled decisions

Do not reopen these. The reasons are in docs/research/ and docs/adr/.

- Issue 338 replaces server-only execution, approved by the owner on
  2026-09-09. PTYs and shells run on each participant's own computer, using
  that person's local account, files, installed tools, and credentials.
- Process model: one trusted room broker plus one local runtime per person
  per room. The broker owns identity, shared routing, grants, and metadata.
  Each local runtime owns its PTYs and agent child processes. Local input
  and process lifetime do not depend on the room connection or broker.
- Seer becomes a native GPUI desktop app for macOS and Linux. Build new
  user facing features in the window. The ratatui TUI stays and gets bug
  fixes until the window replaces it. Keep core types frontend-neutral.
  See ADR 0010 and
  docs/design/04-gpui-feature-checklist.md.
- Linux and macOS only, as host and as client. No Windows.
- Seer is not a herdr replacement. Do not build herdr parity features unless
  a ticket asks for one.
- Reference clones at /home/nethum/Projects/_research/herdr and
  /home/nethum/Projects/_research/luvus are read only. Never modify them.

## Quality gates

These gates apply to all Rust code in this repository. Check them yourself
before every PR.

- Cognitive complexity per function: less than 22 (clippy).
- Cyclomatic complexity per function: less than 22.
- Lines per file: less than 500.
- Dead code: 0. No unused functions, no unused dependencies (cargo-udeps).
  Do not hide dead code with allow attributes.
- Redundant code: 0. Search for an existing helper before you write a new
  one. Do not duplicate logic.
- Escape hatches: no unwrap or expect outside tests. Every allow attribute
  needs a one-line reason. Every unsafe block needs a safety comment.
- cargo clippy with warnings denied must pass. cargo fmt must pass.

## Testing

- Tests are a cost. They slow every change. Default is no test.
- Write a test only to guard a user facing contract or a bug that came
  back. Write the test before the code. Never add a test after the code
  it covers.
- Do not write a test to cover lines, to reach a coverage number, or to
  satisfy a tool.
- Do not write a test that pins an internal detail: a private function,
  a data layout, a log line, an exact error string.
- When you touch a module, delete tests that do not earn their place:
  tests that pin internals, tests that duplicate another test, tests that
  exist only for coverage. A test does not stay because it passes.
- Keep test helpers small. Do not build a test framework.

## Comments

- Comments are a cost. Default is no comment.
- Keep only three kinds: the safety comment on an unsafe block, the reason
  on an allow attribute, and a fact the reader cannot get from the code
  (a protocol quirk, an OS limit, a decision from docs/research/).
- Delete comments that restate the code, that say what a function does
  when its name already says it, or that mark sections.
- Do not write doc comments on private items. Write a doc comment on a
  public item only when the name is not enough.

## Git

- Never push to main. Create a branch, push with git push -u origin, then
  open a PR.
- Run cargo clippy and cargo fmt before every push.
- Add a DCO Signed-off-by line to every commit.
- Never add co-author attribution of any kind.
- Review the staged file list before committing. Drop unrelated files and
  scratch artifacts.

## Writing style

- Use ASD-STE100 Simplified Technical English in comments, docs, commits,
  and PR text.
- Plain ASCII only. No emojis, no em dashes, no decorative formatting.
- Be simple, brief, and direct. Write for a non-native English speaker.

## UI work

These rules apply to every change that a person can see in the desktop
window: layout, spacing, color, type, icons, focus, hover, and motion.

- Use two skill sets, and read the full skill file again each time you use
  it. A read from earlier in the session does not count.
  - Emil Kowalski: https://github.com/emilkowalski/skills, local clone at
    /home/nethum/Projects/_research/emil-skills.
  - Jakub Krehel: https://github.com/jakubkrehel/skills (the better-* skills)
    and https://github.com/jakubkrehel/make-interfaces-feel-better, local
    clones at /home/nethum/Projects/_research/jakub-skills and
    /home/nethum/Projects/_research/jakub-make-interfaces-feel-better.
- Read them before you start a UI surface, before each UI commit, and before
  you report done. The local clones are read only. Pull them for the latest
  copy.
- The design target is docs/design/03-gpui-window.html.
- No eyebrow headers. Do not put a small label (often upper case or letter
  spaced) above a title or a section.
- Spacing is a first class concern. Use one spacing scale. Check every
  padding, gap, and alignment against the design and against the neighbor
  elements.
- Check at pixel level. Run the app, take screenshots of small regions (one
  row, one button, one card edge), zoom in, and find what is wrong: a 1 px
  shift, uneven padding, a color step, a clipped glyph. Fix it and check
  again.
- When the window matches the design, continue to polish. Done means no
  defect is left that you can find in a close screenshot.
