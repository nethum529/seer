# ADR 0008: The people view is a top right picker

Status: accepted on 2026-09-09. Supersedes ADR 0006.

## Context

Wave one gave the client a people drawer, then a people column, then an
overlay panel with a left edge handle and a pin. Every one of those
took space or attention from the terminal, and the closed control at
the top right showed an access word ("Your terminal", "Can type",
"Read only") instead of the person on screen. The owner approved a new
design as a HTML mock on 2026-09-09.

## Decision

- The terminal keeps the whole window. The left handle, the people
  panel, the pin, and the people search are removed.
- The closed control at the top right shows "Seer" and the name of the
  person on screen, the owner's own name included. It shows no access
  word.
- A left click opens the picker. It has a permission line, the people,
  and the server address. Nothing else. The permission line has a green
  or red dot and reads "Permissions granted for NAME" or "Permissions
  not granted for NAME". It means: the local viewer may type in that
  person's terminals. Own terminals are always allowed.
- People fill five rows per column. More people add a column to the
  left. The picker takes the size of the real names and count, and
  always fits the window. When it cannot show every person, it shows
  the columns that fit with a "+N more" cue and the wheel scrolls the
  columns.
- The host has a "host" mark. Person.host carries the flag from the
  broker registry field is_owner.
- A left click on a person shows that person's terminals and updates
  the control. A right click on a person opens the person menu with
  the grant toggle.
- A right click on the control opens the session panel, which keeps
  the terminal, invite, back, and quit actions. No keyboard shortcut
  and no prefix reaches Seer from the terminal.

## Consequences

- The terminal gains the column that the pinned sidebar took.
- Session actions need a right click. The help text, the README, and
  the first run block name it.
- The quit dialog is removed. No path opened it after the people panel
  went away. The session panel keeps the quit row.
- Person gains a host field. Older brokers send no field and clients
  mark no host, because the field defaults to false.
