# Room screens: answers to the open Unknowns

Mockup: docs/design/05-room-screens.html. Open it in a browser. The bar above
the window picks a state. It is mockup chrome, not product. A state also has
a link, for example 05-room-screens.html#invite/ready.

Every action in the mockup maps to a CLI command that exists today:
seer invite, join, list, leave, stop, start, and start --restore.

## R-454 invite sheet

- Where the Invite control lives: in the room menu, as "Invite a person".
  Only the host sees it, because the broker refuses a guest ("owner access
  required"). No second button in the sidebar.
- Lifetime: choose 1 hour, 1 day, 7 days, or Custom (1 to 168 hours). This
  maps to seer invite --hours N. The default is 1 day in the sheet. The CLI
  default is 1 hour.
- Result: two lines, each with Copy. The seer join line is for a friend who
  has Seer. The install line is for a friend who does not. Both carry the
  same one use capsule. The sheet says "Works once. Expires in N."
- List and cancel open invites: the broker cannot do this today. No protocol
  message exists, and seats stay valid across a stop and a later start. So
  the sheet shows no list and no Cancel. This is an owner question.

## R-455 join a room

- First run with no room: the window has no sidebar and shows "Join a
  room" with one field. The field takes the full seer join line, the
  install line, or the bare SEER2 capsule. The app uses the last word that
  starts with SEER, like the CLI.
- Name step: the field holds the system user name ($USER, then $LOGNAME).
  The name is selected, so Enter accepts it and typing replaces it.
- Errors, under the field, with aria-describedby:
  - Bad line (parse fails on this computer): "This is not a Seer invite.
    Paste the full line from the host."
  - Used or expired: the broker gives "invalid seat" for both, so the app
    cannot tell them apart. One message: "This invite was used or has
    expired. Ask the host for a new one."
  - Room cannot be reached (connect fails after the 5 s network timeout):
    "Seer cannot reach the room. Check that it is running, then try again."
    The button becomes Try again.
  - Name in use, on the name step: "That name is in use. Choose another."
- Link handler: register a seer URL scheme. macOS: CFBundleURLTypes in the
  app Info.plist. Linux: a .desktop file with
  MimeType=x-scheme-handler/seer, set with xdg-mime. An opened link skips
  the paste step and lands on the name step.
- Credential privacy: the link carries the same one use capsule as the
  pasted line. It is used once and expires, so a leaked old link is dead.
  Proposal: the app does not show the capsule after join and does not log it.
- Relation to R-432: this mockup uses the capsule as is. A short code needs
  a lookup service, which R-432 must decide first. When R-432 lands, the
  same field and the same link handler accept the short code.

## R-456 room menu

- Trigger: the room name in the sidebar footer (from the 03 design) is now
  a button. The menu opens up from it. On a narrow window the button moves
  to the top bar.
- In the menu: the current room with its state and people count, then the
  other saved rooms with the state from seer list (read only), then the
  actions for your role:
  - Host, room running: Invite a person, Stop room.
  - Host, room stopped: Restore room (start --restore), Start a new room
    (start).
  - Guest: Leave room.
- In settings, not this menu: nothing room related for now. All room
  actions are about this one room, so they stay in the room menu.
- Confirms: Stop, Leave, and Start a new room each open a confirm sheet.
  The body says who is affected. Cancel has focus.
  - Stop: "4 people lose the room. Their own terminals keep running. Your
    terminals on this computer stop too."
  - Leave: "Your terminals on this computer stop. The room stays open for
    the others."
  - Start a new room: "The 4 people of seer-dev are removed."
    The invite sheet belongs to R-454.
- How Stop shows to the others: their own terminals keep running. The
  terminals of other people go away. A bar says "Room offline. Your
  terminals keep running." When the room comes back it says "Room back."
  They cannot tell a stop from a network loss. The bar copy is from the TUI
  today. The banner itself belongs to R-458.
- Switch room: no command switches the current saved room today, so the
  other rooms are read only. This is an owner question.

## Open questions for the owner

1. The broker cannot list or cancel open invites. Add that, or keep invites
   fire and forget?
2. No command switches the saved room. Should the room menu switch rooms?
   That needs new CLI and runtime support.
3. Used and expired invites give the same error. Is one message fine?
4. Invite lives only in the room menu. Do you also want a button in the
   sidebar when the host is alone?
5. The sheet default is 1 day, the CLI default is 1 hour. Which one?
