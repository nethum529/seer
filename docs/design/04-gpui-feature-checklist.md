# GPUI window feature checklist

Status: made for ADR 0010 on 2026-10-07. Sources: README.md and the
output of seer help, built from main at 849be81 (version 0.6.0).

This list has one row for each user facing feature in the README and
in seer help. It says where the feature goes in the desktop window.
Every command in seer help stays a command during and after the move.
See ADR 0010.

The approved design is [03-gpui-window.html](03-gpui-window.html).

Each row starts with one of these words:

- Placed: the approved design gives the place. The item builds it.
- No place yet: the item decides the place.
- Command only: the feature stays a command. The row gives the reason.
- Not needed: the window design removes the need. The row gives the
  reason.
- No control: a rule or a behavior with nothing to click.

Items: T-448 (#448) first window, T-464 (#464) sidebar and terminal
frame, R-449 (#449) input, R-450 (#450)
person dropdown, R-451 (#451) grants, R-452 (#452) New terminal,
R-453 (#453) settings, R-454 (#454) invite sheet, R-455 (#455) join
from a link, R-456 (#456) room menu, R-457 (#457) packaging, R-458
(#458) status banners, R-459 (#459) import from the terminal app.

When a PR decides or builds a row, it updates the row.

## Open and look

| Feature | Source | Window place, or reason | Item |
| --- | --- | --- | --- |
| seer and seer attach open the people and terminals screen | README Commands, help | Placed: the desktop window. The commands stay. | T-448 |
| Your own terminal shows live, with the colors that the program sets | README Open Seer | Placed: the main area. The terminal fills the main area. The owner removed the card. | T-448, T-464 |
| Your terminals use your own files, tools, shell, and logins | README intro | No control. The shell starts as your login shell, also when the app starts from the Dock or a desktop launcher. | T-448 |
| Type or paste into the selected terminal | README Open Seer, help Main screen | Placed: the selected terminal and the floating input bar. | R-449 |
| All keys go to the terminal. Seer has no prefix key | README Open Seer, help Main screen | Placed: keyboard focus in the terminal area. R-449 decides which keys the window keeps for itself. The mockup uses Ctrl T, or Cmd T on macOS, for New terminal, and shells also use Ctrl T. | R-449 |
| Select a terminal: click it, or j, k, and Enter in the session panel | README Open Seer, help | Placed: the vertical tabs in the sidebar. A press on a tab selects it. R-449 decides the keys. | T-464 for own terminals, R-450 for others |
| Drag to select and copy text | README Open Seer | Placed: text selection in the terminal area. The terminal has no card, so R-449 decides where Copy and More go. R-449 decides how a drag shares the mouse with the program. | R-449 |
| Terminal name from the foreground program. A shell is "shell" and "idle". Other programs are "busy" | README People | Placed: the tab name in the sidebar and the title above the terminal. The state dot in the sidebar. | T-464 for the name, R-450 for the dot |
| The back row in the session panel returns to the overview | help | Not needed: the sidebar tabs stay on screen, so the window has no separate overview. | none |
| Quit Seer: the quit row in the session panel, or q | README People, help | Placed: close the window. The shells continue. | T-448 |
| The session panel: right click the top right control | README People, help | Not needed: each row of the panel has its own row in this list. | none |
| Panel and menu keys: j, k, Enter, Esc, Space, n, x, q | help | Not needed: the window uses buttons and the dropdown. The dropdown takes the arrow keys, Enter, and Esc. R-449 decides the window shortcuts. | R-449 |

## People and grants

| Feature | Source | Window place, or reason | Item |
| --- | --- | --- | --- |
| The people picker: left click the top right control | README People, help | Placed: the person dropdown at the top of the sidebar. | R-450 |
| Every person in the room, with the host marked | README People | Placed: the rows of the person dropdown, with "you" and "host" marks. | R-450 |
| Select a person to see their live terminals | README People, help | Placed: pick the person in the dropdown. Their terminals show as vertical tabs. | R-450 |
| Watch the terminals of other people, read only | README intro | Placed: the read only bar under the terminal, with a "My terminals" button. | R-450 |
| Presence and idle time of a person, in the person menu | README People | Placed: the dropdown row shows presence and last seen, for example "away 18m". R-450 decides how agent state shows. | R-450 |
| The terminals of a person in the person menu. Enter to watch | README People, help | Placed: the vertical tabs after you pick the person. | R-450 |
| Your typing permission for the person you watch, in the first line of the picker | README People | No place yet. | R-451 |
| Give or remove the "can type here" grant with Space in the person menu. The box changes when the room server confirms it | README People, help | No place yet. R-451 also decides how the window shows the confirmation. | R-451 |
| With a grant, type and use the mouse in the terminals of another person | README People | Placed: the terminal area of that person, the same as for your own terminal. R-449 builds keys and mouse. R-451 decides how the window shows that you may type. | R-449, R-451 |
| seer perms --on: every person in the room can type into your terminals | README Commands, help | No place yet. The command stays. | R-453 |
| seer perms --off: remove every grant | README People, README Commands, help | No place yet. The command stays. | R-453 |
| seer peek NAME opens Seer with that person selected | README Commands, help | Placed: the command opens the window with that person picked in the dropdown. The command stays. | R-450 |

## Own terminals

| Feature | Source | Window place, or reason | Item |
| --- | --- | --- | --- |
| Open a new terminal: the session panel, or n | README People, help | Placed: the New terminal button in the sidebar. | R-452 |
| Close a terminal: the session panel, or x | README People, help | No place yet. The approved design has no close control. | R-452 |
| Copy the join line from the session panel, and the first run hint that points to it | README People, help First run | No place yet. | R-454 |
| The terminal app must permit clipboard access to copy the join line | README Good to know | Not needed: the window writes to the clipboard itself. | R-454 |

## Rooms

| Feature | Source | Window place, or reason | Item |
| --- | --- | --- | --- |
| seer start opens a new room on Linux and prints a join line. The people of the old room are removed | README Host a room, README Commands, help | No place yet. The command stays. R-456 decides with the room screen. | R-456 |
| seer start --restore reopens the old room with its people | README Commands, help | No place yet. R-456 plans Restore in the room menu. The command stays. | R-456 |
| seer invite --hours N makes a one use join line that lasts 1 to 168 hours | README Host a room, README Commands, help | No place yet. The command stays. | R-454 |
| seer join LINE, or seer join with no line to paste it at a prompt | README Join a room, README Commands, help | No place yet. The command stays. | R-455 |
| The name prompt at join. Enter accepts the system user name | README Join a room | No place yet. | R-455 |
| The install line that installs Seer and joins in one step | README Join a room | No place yet. | R-455 |
| seer list lists your saved rooms and people | README Commands, help | Placed in part: the sidebar footer shows the current room, and the dropdown shows its people. No place yet for the other saved rooms. The command stays. | R-456 |
| The room address in the picker | README People | Placed: the sidebar footer shows the room name and address. | R-456 |
| seer leave removes you from the room and stops your terminals on this computer | README Commands, help | No place yet. R-456 plans Leave in the room menu. The command stays. | R-456 |
| seer stop stops the room server. Host only | README Commands, help | No place yet. R-456 plans Stop room in the room menu. The command stays. | R-456 |

## Processes and updates

| Feature | Source | Window place, or reason | Item |
| --- | --- | --- | --- |
| seer detach disconnects a selected client. Its terminals continue | README Commands, help | Command only. It acts on a different window or TUI. To detach this window, close it. | none |
| seer exit leaves Seer from inside a Seer terminal | README Commands, help | Command only. It runs inside a terminal. In the window, close the window. | none |
| seer ps shows your Seer processes on this computer | README Commands, help | No place yet. The command stays. | R-453 |
| seer ps --clean stops the windows that have no terminal | README Commands, help | No place yet. The command stays. | R-453 |
| seer ps --stop PID stops one runtime with its shells | README Commands, help | No place yet. The command stays. | R-453 |
| seer update installs the latest release | README Commands, help | No place yet. The command stays. R-457 decides how the app updates itself. R-458 shows an Update action. | R-457, R-458 |
| seer restart, or seer restart --yes, restarts your terminals on the installed Seer | help | No place yet. The command stays. It belongs with the self update. | R-457 |
| A different version prints a message that tells you to run seer update | README Good to know | No place yet. A banner with an Update action. | R-458 |
| seer help | help | Command only. The help lines about the TUI keys and the top right control go away with the TUI. | none |
| The install line puts the binaries in ~/.local/bin without sudo | README Install | Command only for the CLI. R-457 decides how the desktop app installs. | R-457 |

## Rules and behavior

| Feature | Source | Window place, or reason | Item |
| --- | --- | --- | --- |
| Your shells continue when you close the window or lose the room connection | README Good to know | Placed: closing the window keeps the shells. No place yet for the banner when the connection drops. | T-448, R-458 |
| A restart of the computer or the runtime restores the saved layout with new shells | README Good to know | No control. The window shows the restored terminals as tabs. | none |
| One computer at a time can publish the terminals of one person | README Good to know | No place yet. The second computer must show the refusal. | R-458 |
| A grant is for one person and one direction. No person can open, close, select, or resize your terminals | README People | No control. The broker and the runtime enforce it. | none |
| Remote connections are encrypted from end to end. A join line gives access to the room only | README Good to know | No control. ADR 0002 does not change. | none |
| The host is trusted. The host computer moves the data of the room | README Good to know | No control. ADR 0009 does not change. | none |
| Seer controls use Catppuccin Mocha with truecolor, and 16 colors if not | README Good to know | Placed: the gpui-component Default Dark palette of the approved design replaces them. | T-448 |
| Terminal output keeps the colors of your own terminal | README Good to know | No place yet. The window draws the terminal itself. | R-459 |

## Not in this list

- There is no chat. The agent inbox is planned and not built. Neither
  is a current feature.
- Known faults, for example issue 436, are bugs and not features.
- Build from source is for contributors.
- Linux and macOS support is in ADR 0010.
