# ADR 0004: One runtime per person under a mapped OS account

Date: 2026-09-04
Status: Accepted

## Context

Before this change, RuntimeManager::spawn started every runtime
with Command::new and changed no UID, no GID, and no login
environment. Every shell and every child process ran as the broker
account, so the per person state directory was not an operating
system boundary. Issue 91 states the gap and names
docs/research/08-multiuser-prior-art.md and
docs/research/09-terminal-heterogeneity.md as the reason a real
boundary is needed.

Issue 91 also fixed the shape of the answer: an explicit table in
broker.toml, no automatic account creation, a switch only when the
broker process has the rights, and an unsafe mapping that fails
closed for that person while the broker keeps serving the others.
Never call sudo. PR 192 (branch feat/91-os-identity) did the work.
Its review asked that this deployment model get its own ADR instead
of a section in ADR 0002. This is that ADR.

## Decision

### The os_users map

- broker.toml has an optional [os_users] table. Each key is an
  exact person name. Each value is an existing OS account name. See
  broker.example.toml. The field is Config::os_users, a
  HashMap<String, String> with serde default, so the table may be
  absent. Seer never creates OS accounts.
- seer start writes no os_users entry. The owner runs as the broker
  account until the operator adds a mapping.
- RuntimeManager::identity looks up the person name in os_users.
  With no entry it returns the default identity, which
  OsIdentity::resolve_process_account builds from the broker process
  account and marks inherit_process. An inherited identity applies
  no environment change and no credential switch: the runtime keeps
  the broker environment and the broker account login shell.

### Refusals

- A mapped account whose resolved uid is 0 is refused with
  PermissionDenied. The message names the person and the account.
- OsIdentity::resolve refuses an account name that is empty, longer
  than 64 bytes, starts with "-", or holds a byte outside ASCII
  alphanumeric, "-", "_", and ".". It also refuses an unknown
  account, a record that does not parse, and a home or login shell
  that is not an absolute path.
- check_switch_rights refuses when a switch is needed and the
  broker process uid is not 0.
- Every refusal is per person: the broker sends Refused after
  Welcome and closes that connection, and other people keep
  working. A peek at a person with a bad mapping drops the peeking
  client.

### The switch in the child

OsIdentity::apply sets HOME, USER, LOGNAME, and SHELL from the
account record, and removes BASH_ENV, ENV, ZDOTDIR, and the five
XDG_* variables so the new account does not inherit broker paths.
It then registers a pre_exec closure on the child that calls
setgroups, then setgid, then setuid. The order matters: after
setuid the process can no longer set groups or the GID. The code
uses libc declarations, not the std CommandExt uid and gid helpers,
because the std path does not set supplementary groups.

needs_switch returns false when the broker uid and gid already
match the target and the broker is not root. A non root broker
cannot change its session groups, so a group difference between the
live session and the account database must not fail that case.

### Directories

- Each person state directory is state_dir/users/<user id>, mode
  0700 and chowned to the mapped account. prepare_directory reads
  it back with symlink_metadata and refuses a symlink, a non
  directory, or wrong ownership.
- The shared state_dir/users directory is 0700 and owned by the
  broker. When the mapped account does not own it,
  allow_identity_traversal sets it to 0711, so the account can
  reach its own directory by path but cannot list the other names.

### The runtime socket

- runtime_directory_path picks XDG_RUNTIME_DIR/seer only when that
  variable is set and non empty and the broker uid equals the
  target uid. In every other case it uses state_dir/seer-<target
  uid>. The socket never lands in a world writable root such as
  bare /tmp, where a local user could pre-create or symlink it.
- The socket path must be 99 bytes or fewer. A longer path is
  refused. The reason for the exact 99 is not recorded in issue 91
  or PR 192; the code only enforces the limit.

### Linux and macOS

- account_fields has two branches. Linux reads
  getent passwd <name> and takes fields 3, 4, 6, and 7. macOS reads
  dscacheutil -q user -a name <name> and takes the uid, gid, dir,
  and shell labels. Groups come from id -G <name> on both.
- set_supplementary_groups also has two branches, because the
  setgroups size argument is size_t on Linux and int on macOS.
- Linux is the supported broker deployment. macOS is a client
  platform: macOS clients attach to a Linux broker and use the
  identities on that server. A macOS broker is not supported.

## Consequences

- The broker needs /usr/bin/id and, on Linux, /usr/bin/getent, plus
  a passwd entry for its own uid, even with an empty os_users
  table. A broker in a container with an arbitrary uid does not
  start.
- Every connect and every People listing resolves identities again,
  which spawns id and getent and may chown directories. A read
  query changes the filesystem. This is left for later.
- The broker shell config key is gone. Runtimes use the mapped
  account login shell. Old config files still parse and ignore the
  key, because the config does not deny unknown fields. An account
  with nologin as its shell gives dead panes.
- A multi user broker must run as a system service with the rights
  to set groups, GID, and UID. A broker without those rights serves
  only people who map to its own account or have no mapping.
