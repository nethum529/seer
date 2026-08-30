# Herdr extensibility and compatibility contract

## Scope and evidence

This document defines the contract our tool must meet to run the current Herdr
skill and Herdr plugins without source changes.

Source paths use these prefixes:

- `herdr/` means `/home/nethum/Projects/_research/herdr`.
- `luvus/` means `/home/nethum/Projects/_research/luvus`.

The Herdr reference clone identifies itself as version 0.8.2. The package also
ships the Herdr skill in the same release artifact. (`herdr/Cargo.toml:1-16`)

The installed skill and the reference skill are not identical. The reference
skill defines blocked startup and blocked prompt errors. The installed skill
does not define those two checks. (`herdr/skills/herdr/SKILL.md:120-130`,
`/home/nethum/.claude/skills/herdr/SKILL.md:120-130`)

Compatibility should therefore be versioned. This document uses the reference
clone as the primary contract. It calls out the installed-skill difference where
it affects behavior.

## Component: Herdr skill control surface

### Name

Herdr control skill.

### Responsibility

The skill teaches a coding agent to inspect topology, create panes, start other
agents, send work, wait for state, and read terminal output. It requires
`HERDR_ENV=1` before any control command. (`herdr/skills/herdr/SKILL.md:8-18`)

The skill treats workspaces, tabs, panes, and agents as different primitives. An
agent runs inside an existing pane. `agent start` does not create layout.
(`herdr/skills/herdr/SKILL.md:46-56`)

### Key files

- `herdr/skills/herdr/SKILL.md:1-195` is the user-facing automation contract.
- `herdr/src/cli.rs:738-767` defines normal JSON output and error exit behavior.
- `herdr/src/api/schema/response.rs:24-44` defines the response envelope.
- `herdr/src/api/schema/response.rs:54-204` defines the response result variants
  used by the skill.

### Key types

- `SuccessResponse` is `{id, result}`. `ResponseResult` has a snake-case `type`
  discriminator. (`herdr/src/api/schema/response.rs:24-44`)
- `ErrorResponse` is `{id, error: {code, message}}`.
  (`herdr/src/api/schema/response.rs:30-40`)
- `AgentStatus` is `idle`, `working`, `blocked`, `done`, or `unknown`.
  (`herdr/src/api/schema/common.rs:149-157`)
- `PaneReadSource` is `visible`, `recent`, `recent_unwrapped`, or `detection`.
  `PaneReadFormat` is `text` or `ansi`.
  (`herdr/src/api/schema/common.rs:55-84`)

### Exact commands and flags used by the skill

The installed binary is the syntax authority. The skill first runs `herdr
--help`, then runs a bare command group to print group help. It must not run bare
`herdr`, because that enters the TUI. (`herdr/skills/herdr/SKILL.md:20-44`)

| Purpose | Exact command shape required by the skill | Source |
| --- | --- | --- |
| Environment gate | `test "${HERDR_ENV:-}" = 1` | `herdr/skills/herdr/SKILL.md:10-16` |
| Top-level discovery | `herdr --help` | `herdr/skills/herdr/SKILL.md:20-26` |
| Group discovery | `herdr agent`, `herdr pane`, `herdr workspace`, `herdr tab`, `herdr worktree`, `herdr terminal`, `herdr notification`, `herdr integration`, `herdr session` | `herdr/skills/herdr/SKILL.md:28-40` |
| Workspace discovery | `herdr workspace list` | `herdr/skills/herdr/SKILL.md:78-86` |
| Tab discovery | `herdr tab list --workspace ID` | `herdr/skills/herdr/SKILL.md:78-86` |
| Current pane | `herdr pane current --current` | `herdr/skills/herdr/SKILL.md:78-86` |
| Pane discovery | `herdr pane list --workspace ID` | `herdr/skills/herdr/SKILL.md:78-86` |
| Agent discovery | `herdr agent list` | `herdr/skills/herdr/SKILL.md:78-86` |
| Geometry | `herdr pane layout --pane ID` | `herdr/skills/herdr/SKILL.md:94-100` |
| Split | `herdr pane split --current --direction right|down --cwd PATH --no-focus` | `herdr/skills/herdr/SKILL.md:100-106` |
| Start agent | `herdr agent start NAME --kind KIND --pane ID [-- NATIVE_ARGS...]` | `herdr/skills/herdr/SKILL.md:108-120` |
| Prompt agent | `herdr agent prompt TARGET TEXT --wait --timeout MILLISECONDS` | `herdr/skills/herdr/SKILL.md:122-130` |
| Wait for state | `herdr agent wait TARGET [--until STATE] [--timeout MILLISECONDS]` | `herdr/skills/herdr/SKILL.md:132-138` |
| Send logical keys | `herdr agent send-keys TARGET esc|ctrl+c` | `herdr/skills/herdr/SKILL.md:140-147` |
| Inspect agent | `herdr agent get TARGET` | `herdr/skills/herdr/SKILL.md:147-154` |
| Read agent | `herdr agent read TARGET --source recent-unwrapped --lines N` | `herdr/skills/herdr/SKILL.md:147-154` |
| Run command | `herdr pane run PANE_ID TEXT` | `herdr/skills/herdr/SKILL.md:156-172` |
| Wait for output | `herdr pane wait-output PANE_ID --match TEXT|--regex REGEX [--timeout MILLISECONDS]` | `herdr/skills/herdr/SKILL.md:166-172` |
| Read pane | `herdr pane read PANE_ID --source SOURCE --lines N [--format text|ansi]` | `herdr/skills/herdr/SKILL.md:166-183` |

The skill also reasons about `workspace create`, `tab create`, and `pane split`
responses when a requested topology change needs them. The create result variants
are `workspace_created` with `workspace`, `tab`, and `root_pane`, and
`tab_created` with `tab` and `root_pane`.
(`herdr/skills/herdr/SKILL.md:88-92`,
`herdr/src/api/schema/response.rs:57-64`,
`herdr/src/api/schema/response.rs:87-96`)

### Exact JSON fields used by the skill

The skill explicitly names these JSON paths:

| Result | Required path |
| --- | --- |
| Any success | `.id`, `.result.type` |
| Any server error | `.id`, `.error.code`, `.error.message` |
| Workspace create | `.result.workspace`, `.result.tab`, `.result.root_pane` |
| Tab create | `.result.tab`, `.result.root_pane` |
| Pane split | `.result.pane`, especially `.result.pane.pane_id` |
| Pane move | `.result.move_result.previous_pane_id`, `.result.move_result.pane.pane_id` |

The envelope fields come from the response schema. The create and move paths are
also named directly by the skill. (`herdr/src/api/schema/response.rs:24-44`,
`herdr/src/api/schema/response.rs:57-64`,
`herdr/src/api/schema/response.rs:87-130`,
`herdr/skills/herdr/SKILL.md:68-68`,
`herdr/skills/herdr/SKILL.md:88-88`,
`herdr/skills/herdr/SKILL.md:100-106`)

The workflow also reads these typed result objects:

- `workspace_list.workspaces[].workspace_id` and
  `tab_list.tabs[].{tab_id,workspace_id}` identify topology.
  (`herdr/src/api/schema/response.rs:62-64`,
  `herdr/src/api/schema/workspaces.rs:59-73`,
  `herdr/src/api/schema/tabs.rs:40-48`)
- `pane_current.pane`, `pane_list.panes[]`, and split responses use `PaneInfo`.
  The important identity and placement fields are `pane_id`, `workspace_id`,
  `tab_id`, `focused`, `cwd`, `agent`, and `agent_status`.
  (`herdr/src/api/schema/response.rs:117-125`,
  `herdr/src/api/schema/panes.rs:447-480`)
- `pane_layout.layout` supplies the area, pane rectangles, and focused pane. The
  skill uses this to choose `right` or `down`.
  (`herdr/src/api/schema/response.rs:135-137`,
  `herdr/src/api/schema/panes.rs:588-620`,
  `herdr/skills/herdr/SKILL.md:94-106`)
- `agent_info.agent`, `agent_list.agents[]`, `agent_started.agent`, and
  `agent_prompted.agent` use `AgentInfo`. Its coordination fields include
  `terminal_id`, `name`, `agent`, `agent_status`, `workspace_id`, `tab_id`,
  `pane_id`, `focused`, `launch_pending`, `interactive_ready`, and
  `state_change_seq`. (`herdr/src/api/schema/response.rs:97-109`,
  `herdr/src/api/schema/agents.rs:183-223`)
- `pane_read.read` supplies `pane_id`, `source`, `format`, `text`, `revision`, and
  `truncated`. `output_matched` adds `pane_id`, `revision`, `matched_line`, and a
  nested `read`. (`herdr/src/api/schema/response.rs:162-164`,
  `herdr/src/api/schema/response.rs:199-204`,
  `herdr/src/api/schema/panes.rs:674-684`)
- `pane_move.move_result` supplies the old IDs, current `pane`, source and target
  layouts, created or closed topology, and the focused pane ID.
  (`herdr/src/api/schema/panes.rs:537-558`)

Normal control commands serialize one-line JSON to stdout. Server errors serialize
JSON to stderr and return status 1. Syntax errors return status 2. `pane read` and
`agent read` are exceptions: they print only `result.read.text` on success.
(`herdr/src/cli.rs:84-92`, `herdr/src/cli.rs:738-767`,
`herdr/skills/herdr/SKILL.md:187-195`)

### Lifecycle behavior

Agent names match `[a-z][a-z0-9_-]{0,31}`. A target is a unique live name or the
current pane ID. It is not a terminal ID or a bare kind label.
(`herdr/skills/herdr/SKILL.md:54-58`)

The reference skill requires `agent start` to wait for the expected agent and
interactive readiness. Its default startup timeout is 30 seconds. A blocked
startup returns `agent_not_ready` but leaves the assigned name usable.
(`herdr/skills/herdr/SKILL.md:108-120`)

The reference skill requires `agent prompt` to reject an already blocked agent
with `agent_blocked`. With `--wait`, it settles on the first `idle`, `done`, or
`blocked` state. A prompt that does not cause an observed state change within five
seconds returns `agent_prompt_stalled`. (`herdr/skills/herdr/SKILL.md:122-138`)

The installed skill does not state the blocked-start or blocked-prompt checks.
UNKNOWN: whether every deployed Herdr binary implements the reference behavior.
Resolve this with a versioned CLI compatibility test against each supported
binary. (`herdr/skills/herdr/SKILL.md:120-130`,
`/home/nethum/.claude/skills/herdr/SKILL.md:120-130`)

### How it talks to other components

The skill calls the Herdr CLI. The CLI selects the current server, sends typed
socket requests, and returns schema-defined JSON. Plugins use the same CLI and
socket API. (`herdr/src/cli.rs:762-774`,
`herdr/docs/next/website/src/content/docs/plugins.mdx:18-29`)

## Component: Pane environment and public identity

### Name

Managed pane context.

### Responsibility

This component gives every pane enough information to address itself and the
server that created it. It also gives the skill stable public handles.
(`herdr/src/pane.rs:130-152`, `herdr/skills/herdr/SKILL.md:60-76`)

### Key files

- `herdr/src/pane.rs:95-152` constructs pane launch environment.
- `herdr/src/integration/env.rs:8-31` defines identity, socket, and binary values.
- `herdr/src/workspace.rs:106-204` creates public IDs and tracks reuse.
- `herdr/src/app/ids.rs:60-150` resolves current IDs and legacy aliases.
- `herdr/src/app/api/panes.rs:830-840` records an alias after a cross-workspace
  move.

### Key types

- `PaneLaunchEnv` contains caller-supplied extra values and either inherited,
  managed, or omitted pane identity. (`herdr/src/pane.rs:95-128`)
- Workspace IDs use `w` plus Herdr's readable base-32 encoding. Pane and tab IDs
  add `:p` and `:t` plus a local public number.
  (`herdr/src/workspace.rs:106-151`)

### Environment contract

Every Herdr pane gets these Herdr-owned values:

| Variable | Value and availability | Source |
| --- | --- | --- |
| `HERDR_ENV` | Always `1` | `herdr/src/pane.rs:130-137` |
| `HERDR_SOCKET_PATH` | Current server socket or named-pipe selector | `herdr/src/integration/env.rs:28-31` |
| `HERDR_BIN_PATH` | Exact running executable, when current executable lookup succeeds | `herdr/src/integration/env.rs:28-32` |
| `HERDR_WORKSPACE_ID` | Managed pane workspace ID | `herdr/src/pane.rs:138-147` |
| `HERDR_TAB_ID` | Managed pane tab ID | `herdr/src/pane.rs:138-147` |
| `HERDR_PANE_ID` | Managed pane public pane ID | `herdr/src/pane.rs:138-150` |
| `HERDR_PANE_RUNTIME_ID` | Internal marker for Windows panes launched through Git Bash | `herdr/src/platform/windows.rs:94-94`, `herdr/src/platform/windows.rs:485-491` |

Caller extra environment is applied first. Herdr-owned values are applied after
it, so callers cannot spoof those values. `CODEX_THREAD_ID` is removed before
launch. (`herdr/src/pane.rs:130-150`)

The three identity values are launch-time context. A process keeps its old
`HERDR_PANE_ID` after a move, so automation must use the move response or a live
agent name. (`herdr/skills/herdr/SKILL.md:60-76`)

`HERDR_SESSION` selects a named session. An explicit CLI `--session` is the first
client socket selector. `HERDR_SOCKET_PATH` is next. The legacy
`HERDR_CLIENT_SOCKET_PATH` is next. The active session directory is the fallback.
(`herdr/src/session.rs:10-29`, `herdr/src/server/socket_paths.rs:4-44`)

`HERDR_AGENT=KIND` is a process-detection hint for wrappers. It only names an
agent kind already known to Herdr. It is not a standard pane identity value.
(`herdr/docs/next/website/src/content/docs/agents.mdx:52-56`)

UNKNOWN: whether arbitrary inherited parent environment is a supported contract
or an implementation detail. The source explicitly guarantees Herdr-owned and
caller-extra values, but it does not define a stable allowlist for all inherited
variables. Resolve this with a public environment specification and cross-platform
tests. (`herdr/src/pane.rs:130-152`)

### ID stability and reuse

Workspace handles are process-global monotonic public IDs. Restored workspaces
advance the allocator past the largest restored handle.
(`herdr/src/workspace.rs:106-175`)

Pane and tab public numbers are stable inside one workspace. Closed numbers are
not reused. Source tests cover both cases.
(`herdr/src/workspace.rs:201-204`, `herdr/src/workspace.rs:1571-1613`)

A pane moved across workspaces gets a new workspace-qualified public ID. Herdr
stores the previous public ID as an alias, but agent targeting requires the
current public ID. (`herdr/src/app/api/panes.rs:830-840`,
`herdr/src/app/ids.rs:106-150`,
`herdr/src/app/api/panes.rs:978-992`)

### How it talks to other components

The skill reads identity values and sends them back as CLI targets. The CLI and
socket schema return replacement IDs after mutations. Plugin commands receive the
same context when it is available. (`herdr/skills/herdr/SKILL.md:60-88`,
`herdr/src/app/api/plugins/runtime.rs:64-81`)

## Component: Agent kind registry and detection

### Name

Compiled agent registry plus screen manifests.

### Responsibility

This component maps kind labels to executables, recognizes foreground processes,
and classifies the live screen as idle, working, blocked, or unknown.
(`herdr/src/detect/mod.rs:121-221`, `herdr/src/detect/mod.rs:237-310`)

### Key files

- `herdr/src/detect/mod.rs:41-221` contains the closed kind enum, labels,
  executables, and aliases.
- `herdr/src/detect/manifest.rs:138-261` defines and bundles screen manifests.
- `herdr/src/detect/manifest.rs:599-688` selects override, remote, or bundled
  rules.
- `herdr/docs/next/website/src/content/docs/agents.mdx:40-78` defines detection
  authority and update behavior.

### Key types

- `Agent` is a compiled enum with 23 entries. `ALL` defines the supported kind
  list. `SCREEN_MANIFEST_AGENTS` defines which kinds can use screen rules.
  (`herdr/src/detect/mod.rs:41-119`)
- `AgentManifest` has `id`, version gates, aliases, and rules. A rule can set a
  state and match text or regular expressions in defined regions.
  (`herdr/src/detect/manifest.rs:138-221`)

### What adding an agent kind requires

A new kind requires a binary change. The author must add the enum entry and list
membership, canonical label, executable, accepted aliases, and process identity
mapping. (`herdr/src/detect/mod.rs:41-221`)

If the kind uses screen detection, it also needs a bundled manifest and inclusion
in `SCREEN_MANIFEST_AGENTS`. (`herdr/src/detect/mod.rs:96-118`,
`herdr/src/detect/manifest.rs:239-261`)

Remote and local manifests can replace detection rules only for an existing kind.
They cannot add process recognition, labels, or integration behavior for a new
kind. (`herdr/docs/next/website/src/content/docs/agents.mdx:64-78`)

### How it talks to other components

Foreground process detection selects an `Agent`. The selected integration or
screen manifest supplies lifecycle state. The agent CLI exposes that result as
`AgentInfo.agent` and `AgentInfo.agent_status`.
(`herdr/docs/next/website/src/content/docs/agents.mdx:40-50`,
`herdr/src/api/schema/agents.rs:183-223`)

## Component: Plugin declaration and registry

### Name

Herdr plugin manifest and global registry.

### Responsibility

A plugin is an executable directory. `herdr-plugin.toml` declares metadata,
builds, startup hooks, actions, event hooks, panes, and link handlers. Herdr
launches plain argv commands. (`herdr/docs/next/website/src/content/docs/plugins.mdx:6-21`,
`herdr/src/api/schema/plugins.rs:229-289`)

### Key files

- `herdr/src/app/api/plugins/manifest.rs:118-227` loads and normalizes a
  `herdr-plugin.toml` file.
- `herdr/src/api/schema/plugins.rs:37-113` defines installed plugin and source
  metadata.
- `herdr/src/persist/plugin_registry.rs:11-132` stores and reloads the global
  registry.
- `herdr/src/plugin_paths.rs:5-30` defines managed source, config, and state
  locations.

### Key types

Required top-level manifest fields are `id`, `name`, `version`, and
`min_herdr_version`. Optional top-level fields are `description`, `platforms`,
`build`, `startup`, `actions`, `events`, `panes`, and `link_handlers`.
(`herdr/src/app/api/plugins/manifest.rs:118-225`,
`herdr/src/api/schema/plugins.rs:37-67`)

Actions have `id`, `title`, optional description, contexts, platforms, and argv.
Events have a dot-name, platforms, and argv. Panes have identity, placement,
optional size, and argv. Link handlers map a pattern to an action.
(`herdr/src/api/schema/plugins.rs:229-289`)

The action contexts are `global`, `workspace`, `tab`, `pane`, and `selection`.
Pane placements are `overlay`, `popup`, `split`, `tab`, and `zoomed`.
(`herdr/src/api/schema/plugins.rs:353-361`,
`herdr/src/api/schema/plugins.rs:417-452`)

Supported hook events are workspace create, update, close, rename, move, reorder,
and focus; worktree create, open, and remove; tab create, close, rename, move, and
focus; and pane create, close, focus, move, exit, agent detection, and agent status
change. (`herdr/src/api/schema/events.rs:223-252`,
`herdr/src/api/schema/events.rs:286-309`)

### How it talks to other components

The registry resolves enabled declarations into UI actions, event subscribers,
and pane entrypoints. Plugin commands call the full Herdr CLI or raw socket API.
There is no smaller plugin SDK. (`herdr/docs/next/website/src/content/docs/plugins.mdx:18-33`)

## Component: Plugin distribution and install lifecycle

### Name

GitHub installer and local linker.

### Responsibility

`plugin install` manages GitHub source. `plugin link` registers a local working
tree. Enable, disable, unlink, uninstall, action, log, and pane commands manage
the installed entry. (`herdr/src/cli/plugin.rs:1643-1668`)

### Key files

- `herdr/src/cli/plugin.rs:145-330` implements install and uninstall.
- `herdr/src/cli/plugin.rs:700-859` parses GitHub shorthand and performs the
  shallow detached checkout.
- `herdr/src/cli/plugin.rs:900-1005` registers online or offline and verifies
  persisted source metadata.
- `herdr/src/cli/plugin.rs:1202-1365` previews and runs builds.

### Key types

`PluginSourceInfo` records local or GitHub source, owner, repository, subdirectory,
requested ref, resolved commit, managed path, and install time.
(`herdr/src/api/schema/plugins.rs:70-113`)

### Install and version lifecycle

GitHub syntax is `owner/repo[/subdir...]` with optional `--ref REF` and `--yes`.
Install performs a depth-one fetch and detached checkout. It shows declared code
before confirmation, runs supported build commands, verifies that the build did
not change the manifest, moves the source into managed storage, and registers the
resolved commit. (`herdr/src/cli/plugin.rs:835-859`,
`herdr/src/cli/plugin.rs:1202-1365`,
`herdr/docs/next/website/src/content/docs/plugins.mdx:191-205`)

The manifest `version` only has to be non-empty. `min_herdr_version` must parse as
a semantic version and must not be newer than the running host.
(`herdr/src/app/api/plugins/manifest.rs:144-152`,
`herdr/src/app/api/plugins/manifest.rs:229-258`)

There is no `plugin update` command. Reinstalling refreshes and replaces a
GitHub-managed checkout. Installing over a local link is rejected. Uninstall
removes a managed checkout. Unlink leaves source files in place.
(`herdr/docs/next/website/src/content/docs/plugins.mdx:191-211`)

Build commands do not receive the Herdr runtime or plugin environment. The
installer explicitly removes socket, session, identity, binary, and all
`HERDR_PLUGIN_*` variables. (`herdr/src/cli/plugin.rs:1502-1520`,
`herdr/docs/next/website/src/content/docs/plugins.mdx:218-231`)

### How it talks to other components

The installer uses Git for source distribution, the manifest loader for
validation, and the global registry for activation. It can write the registry
when no server is running. (`herdr/src/cli/plugin.rs:835-911`,
`herdr/docs/next/website/src/content/docs/plugins.mdx:191-200`)

## Component: Plugin marketplace

### Name

GitHub topic index.

### Responsibility

The marketplace discovers public GitHub repositories tagged `herdr-plugin`. It
is an unreviewed index, not a package host or trust authority.
(`herdr/docs/next/website/src/content/docs/marketplace.mdx:6-21`)

### Key files

- `herdr/workers/plugin-marketplace/src/index.ts:3-35` defines the query,
  manifest name, and scan limits.
- `herdr/workers/plugin-marketplace/src/index.ts:208-386` refreshes the index and
  writes its snapshot.
- `herdr/workers/plugin-marketplace/src/index.ts:487-605` scans changed repository
  trees and manifest contents.
- `herdr/workers/plugin-marketplace/src/index.ts:713-763` parses listing metadata.
- `herdr/docs/next/website/src/content/docs/marketplace.mdx:37-54` defines listing
  and refresh behavior.

### Key types

The snapshot stores repository metadata, exact default-branch commit, manifest
path, `id`, `name`, `version`, `platforms`, and `min_herdr_version`.
(`herdr/docs/next/website/src/content/docs/marketplace.mdx:46-54`,
`herdr/workers/plugin-marketplace/src/index.ts:99-154`)

### Discovery and publication lifecycle

Publication requires a public non-fork, non-archived GitHub repository, the topic,
and at least one parseable `herdr-plugin.toml` outside ignored test fixture paths.
The scanner accepts root or nested manifests and de-duplicates repeated plugin IDs
by preferring the shallowest path. (`herdr/workers/plugin-marketplace/src/index.ts:1017-1056`,
`herdr/workers/plugin-marketplace/src/index.ts:1068-1108`)

The index refreshes every 30 minutes and rescans a repository when its default
branch head changes. (`herdr/docs/next/website/src/content/docs/marketplace.mdx:37-44`)

### How it talks to other components

The website reads the generated index. Installation still clones GitHub directly
with `herdr plugin install owner/repo[/subdir...]`.
(`herdr/docs/next/website/src/content/docs/marketplace.mdx:24-35`,
`herdr/src/cli/plugin.rs:835-859`)

## Component: Plugin runtime and trust boundary

### Name

Plugin process host.

### Responsibility

The host runs plugin argv in the plugin root, injects context, captures bounded
stdout and stderr, keeps logs, and limits concurrent commands.
(`herdr/src/app/api/plugins/runtime.rs:11-24`,
`herdr/src/app/api/plugins/runtime.rs:82-180`)

### Key files

- `herdr/src/app/api/plugins/runtime.rs:15-180` launches action, startup, and event
  commands.
- `herdr/src/app/api/plugins/env.rs:3-29` creates plugin-owned directories and
  path environment.
- `herdr/src/api/schema/plugins.rs:363-395` defines invocation context.
- `herdr/src/app/api/plugins/panes.rs:236-269` launches declared pane commands.

### Key types

`PluginInvocationContext` can carry workspace, worktree, tab, focused pane, agent,
status, selection, invocation source, correlation ID, clicked URL, and link
handler ID. (`herdr/src/api/schema/plugins.rs:363-395`)

### Runtime environment

All runtime plugin commands receive `HERDR_ENV=1`, `HERDR_SOCKET_PATH`,
`HERDR_PLUGIN_ID`, `HERDR_PLUGIN_ROOT`, `HERDR_PLUGIN_CONFIG_DIR`,
`HERDR_PLUGIN_STATE_DIR`, and `HERDR_PLUGIN_CONTEXT_JSON`. They receive
`HERDR_BIN_PATH` when executable lookup succeeds. They receive available
workspace, tab, and pane IDs. (`herdr/src/app/api/plugins/runtime.rs:39-72`,
`herdr/src/app/api/plugins/env.rs:15-29`)

Actions add `HERDR_PLUGIN_ACTION_ID`. Startup and event hooks add
`HERDR_PLUGIN_EVENT`. Event hooks add `HERDR_PLUGIN_EVENT_JSON`. Link handlers add
`HERDR_PLUGIN_CLICKED_URL` and `HERDR_PLUGIN_LINK_HANDLER_ID` when applicable.
(`herdr/src/app/api/plugins/runtime.rs:55-81`)

Declared plugin panes also receive `HERDR_PLUGIN_ENTRYPOINT_ID` and then inherit
the standard managed pane environment. (`herdr/docs/next/website/src/content/docs/plugins.mdx:251-261`,
`herdr/src/pane.rs:130-152`)

### Trust model

Plugin code runs as the user. It inherits the environment, can run arbitrary
programs, and can call the full Herdr CLI. Herdr validates the manifest but does
not review or sandbox the code. (`herdr/docs/next/website/src/content/docs/plugins.mdx:35-52`)

The marketplace adds no trust. Its listing is automatic and unreviewed.
(`herdr/docs/next/website/src/content/docs/marketplace.mdx:6-21`)

UNKNOWN: there is no bounded compatibility promise for arbitrary third-party
plugins because the entire CLI and socket API are available to them. A complete
answer requires a versioned plugin API subset, protocol conformance suite, or an
inventory of each plugin's actual calls.
(`herdr/docs/next/website/src/content/docs/plugins.mdx:23-33`)

### How it talks to other components

Manifest declarations create processes. Environment values let those processes
call the exact running binary or socket. Their API calls then use the same
topology, identity, agent, and response components as the skill.
(`herdr/docs/next/website/src/content/docs/plugins.mdx:18-29`,
`herdr/src/app/api/plugins/runtime.rs:39-81`)

## Component: Current Luvus implementation

### Name

Existing target surfaces.

### Responsibility

Luvus already has pane context, public object IDs, a compiled agent descriptor
registry, and executable module manifests. These are analogous to Herdr surfaces,
but their names and shapes are not wire-compatible.
(`luvus/src/terminal/pty.rs:1155-1190`, `luvus/src/ids.rs:1-45`,
`luvus/src/agent/registry.rs:1-61`, `luvus/src/module/manifest.rs:11-50`)

### Key files

- `luvus/src/terminal/pty.rs:1155-1190` injects pane environment.
- `luvus/src/ids.rs:1-45` defines internal pane IDs and opaque public IDs.
- `luvus/src/agent/registry.rs:1-61` defines the compiled agent registry.
- `luvus/src/agent/types.rs:7-52` defines agent identity, session, and integration
  descriptors.
- `luvus/src/module/manifest.rs:11-50` defines executable module declarations.
- `luvus/src/module/runtime.rs:52-96` injects module environment.

### Key types and gaps

Luvus pane children receive `LUVUS_ENV`, a numeric `LUVUS_PANE_ID`, socket and
binary paths, and legacy `BOHAY_*` aliases. They do not receive Herdr workspace,
tab, or public pane identity variables. (`luvus/src/terminal/pty.rs:1155-1190`)

Luvus `public_id` produces a random or fallback `kind_...` identity. Its pane
child context still uses the process-global numeric `PaneId`.
(`luvus/src/ids.rs:1-45`, `luvus/src/terminal/pty.rs:1170-1178`)

Luvus agent support is also compiled. Its descriptor is richer than Herdr's
enum mapping, but new entries still require insertion into `BUILTINS`.
(`luvus/src/agent/registry.rs:1-61`, `luvus/src/agent/types.rs:7-52`)

`luvus-module.toml` is close to `herdr-plugin.toml`. It has identity, version,
builds, startup hooks, actions, events, and panes. It also has docks, bars, and
settings. Its names and runtime variables use `LUVUS_MODULE_*`, not
`HERDR_PLUGIN_*`. (`luvus/src/module/manifest.rs:11-50`,
`luvus/src/module/runtime.rs:52-96`)

### How it talks to other components

The current Luvus skill and CLI use Luvus-native IDs and environment. Existing
modules call back through `LUVUS_SOCKET_PATH` and `LUVUS_BIN_PATH`.
(`luvus/src/terminal/pty.rs:1171-1188`,
`luvus/src/module/runtime.rs:64-95`)

## Compatibility requirements

### MUST for the Herdr skill

- MUST inject `HERDR_ENV=1`, `HERDR_SOCKET_PATH`, `HERDR_BIN_PATH`,
  `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`, and `HERDR_PANE_ID` into managed panes
  with Herdr-owned values taking precedence. (`herdr/src/pane.rs:130-150`,
  `herdr/src/integration/env.rs:28-32`)
- MUST implement every command shape and flag in the skill command table above.
  (`herdr/skills/herdr/SKILL.md:20-40`, `herdr/skills/herdr/SKILL.md:78-183`)
- MUST return the response envelope, result discriminators, result objects, and
  error exit behavior listed above. (`herdr/src/api/schema/response.rs:24-44`,
  `herdr/src/cli.rs:738-759`)
- MUST print raw text for `pane read` and `agent read`, not the JSON envelope.
  (`herdr/src/cli.rs:84-92`)
- MUST support opaque stable workspace, tab, and pane handles. Closed tab and
  pane handles MUST NOT be reused. (`herdr/skills/herdr/SKILL.md:60-68`,
  `herdr/src/workspace.rs:1571-1613`)
- MUST return the new pane ID after a cross-workspace move and continue to resolve
  launch-time caller context as defined by the skill. (`herdr/skills/herdr/SKILL.md:60-76`,
  `herdr/src/app/api/panes.rs:978-992`)
- MUST implement agent name validation, target resolution, readiness, lifecycle
  states, waits, output reads, and status errors used by the reference skill.
  (`herdr/skills/herdr/SKILL.md:54-58`, `herdr/skills/herdr/SKILL.md:108-154`)

### MUST for Herdr plugin source compatibility

- MUST accept `herdr-plugin.toml` with the declaration types and validation rules
  above. (`herdr/src/app/api/plugins/manifest.rs:118-258`,
  `herdr/src/api/schema/plugins.rs:229-289`)
- MUST expose install, uninstall, link, unlink, list, enable, disable, config-dir,
  action, log, and pane CLI commands with Herdr-compatible results.
  (`herdr/src/cli/plugin.rs:1643-1668`,
  `herdr/src/api/schema/response.rs:231-266`)
- MUST run plugin argv in the plugin root and inject every applicable
  `HERDR_PLUGIN_*`, identity, socket, and binary value described above.
  (`herdr/src/app/api/plugins/runtime.rs:39-81`,
  `herdr/src/app/api/plugins/env.rs:15-29`)
- MUST provide a compatible executable at `HERDR_BIN_PATH` and a compatible
  socket protocol for any API advertised to plugins. Plugins are allowed to use
  the whole interface. (`herdr/docs/next/website/src/content/docs/plugins.mdx:23-29`)
- MUST keep config and state outside managed source and preserve them across a
  managed reinstall. (`herdr/docs/next/website/src/content/docs/plugins.mdx:191-211`,
  `herdr/docs/next/website/src/content/docs/plugins.mdx:263-270`)
- MUST preserve the no-sandbox trust model or explicitly reject untrusted Herdr
  plugins. Silent partial sandboxing would change plugin behavior.
  (`herdr/docs/next/website/src/content/docs/plugins.mdx:35-52`)

### SHOULD

- SHOULD expose protocol and feature versions so a skill can select blocked-agent
  semantics instead of inferring them from documentation skew.
  (`herdr/skills/herdr/SKILL.md:120-130`,
  `/home/nethum/.claude/skills/herdr/SKILL.md:120-130`)
- SHOULD preserve Herdr public ID spelling. This is not required if all IDs remain
  opaque, but it helps scripts and diagnostics that validate examples.
  (`herdr/skills/herdr/SKILL.md:60-68`)
- SHOULD provide install preview, ref pinning, resolved commit recording, build
  failure rollback, and manifest-change detection. These reduce supply-chain
  ambiguity. (`herdr/src/cli/plugin.rs:835-859`,
  `herdr/src/cli/plugin.rs:981-1005`,
  `herdr/src/cli/plugin.rs:1202-1365`)
- SHOULD keep agent kinds in one descriptor registry even if Herdr compatibility
  is exposed through adapters. Luvus already centralizes identity, sessions, and
  integrations in a descriptor. (`luvus/src/agent/types.rs:7-52`,
  `luvus/src/agent/registry.rs:42-61`)

### MAY

- MAY add richer module entrypoints such as docks, bars, and settings. They are
  Luvus extensions beyond the Herdr plugin manifest.
  (`luvus/src/module/manifest.rs:38-50`)
- MAY index Herdr-compatible packages in a separate marketplace. Marketplace
  compatibility is not needed to install a known GitHub source.
  (`herdr/docs/next/website/src/content/docs/marketplace.mdx:24-35`)
- MAY keep Luvus-native names in parallel, but a Herdr compatibility mode must
  still expose the exact Herdr variables, CLI, and JSON surface.
  (`luvus/src/terminal/pty.rs:1170-1188`,
  `herdr/src/pane.rs:130-150`)

## Takeaways for our tool

### Copy

- Copy the response envelope and typed result objects. They give skills one
  machine-readable control surface. (`herdr/src/api/schema/response.rs:24-44`)
- Copy stable non-reused public handles and explicit replacement IDs after moves.
  (`herdr/src/workspace.rs:1571-1613`,
  `herdr/src/app/api/panes.rs:978-992`)
- Copy the exact-binary and exact-socket environment. It prevents client and
  server version skew. (`herdr/src/integration/env.rs:28-32`)
- Copy declarative argv plugins, separate source/config/state paths, install
  previews, ref pinning, and resolved commit metadata.
  (`herdr/src/api/schema/plugins.rs:70-113`,
  `herdr/docs/next/website/src/content/docs/plugins.mdx:42-50`)

### Avoid

- Avoid making the entire unversioned CLI and socket surface the permanent plugin
  API. That makes arbitrary plugin compatibility unbounded.
  (`herdr/docs/next/website/src/content/docs/plugins.mdx:23-29`)
- Avoid two skill contracts for one binary line. Publish skill behavior with a
  feature or protocol version. (`herdr/skills/herdr/SKILL.md:120-130`,
  `/home/nethum/.claude/skills/herdr/SKILL.md:120-130`)
- Avoid a closed agent enum as the only registry. Detection rule updates cannot
  add a new kind without a binary release. (`herdr/src/detect/mod.rs:41-119`,
  `herdr/docs/next/website/src/content/docs/agents.mdx:64-78`)
- Avoid treating marketplace presence as review or trust.
  (`herdr/docs/next/website/src/content/docs/marketplace.mdx:6-21`)

### Open questions

- UNKNOWN: Which Herdr release or skill revision is the first promised target?
  Resolve this by selecting supported versions and running their CLI conformance
  suite. (`herdr/Cargo.toml:1-16`,
  `/home/nethum/.claude/skills/herdr/SKILL.md:120-130`)
- UNKNOWN: Do we promise compatibility with all Herdr socket methods, or only the
  skill and declared plugin runtime subset? Resolve this with a versioned public
  API boundary. (`herdr/docs/next/website/src/content/docs/plugins.mdx:23-29`)
- UNKNOWN: Do we preserve Herdr ID spelling or expose an alias layer over Luvus
  public IDs? Resolve this before persistence migration.
  (`herdr/src/workspace.rs:106-151`, `luvus/src/ids.rs:13-39`)
- UNKNOWN: Can a data-driven agent descriptor add a new kind, or will new kinds
  remain compiled? Resolve this with an agent registry design that covers process
  identity, launch command, state detection, sessions, and integrations.
  (`herdr/src/detect/mod.rs:41-221`, `luvus/src/agent/types.rs:7-52`)
- UNKNOWN: Should Herdr plugin compatibility be a mode, a manifest importer, or a
  permanent alias surface? Resolve this with one end-to-end sample plugin that
  installs, opens a pane, handles an event, and calls back through
  `HERDR_BIN_PATH`. (`herdr/docs/next/website/src/content/docs/plugins.mdx:18-33`)
