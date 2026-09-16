use std::collections::BTreeMap;
use std::fmt::{Arguments, Write as _};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::proto::{ClientMsg, ServerMsg};
use crate::{InputEvent, TerminalFrame};

struct Log {
    file: File,
    prefix: String,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
static TRANSITIONS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// Opens `<directory>/<process>.debug.log`. A failure leaves logging off.
pub fn open(directory: &Path, process: &str, participant: &str) {
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let Ok(file) = OpenOptions::new()
        .append(true)
        .create(true)
        .open(directory.join(format!("{process}.debug.log")))
    else {
        return;
    };
    let prefix = format!("{process}[{}] {participant}", std::process::id());
    let _ = LOG.set(Mutex::new(Log { file, prefix }));
    write(format_args!("log open"));
}

pub fn write(args: Arguments<'_>) {
    let Some(log) = LOG.get() else {
        return;
    };
    let Ok(mut log) = log.lock() else {
        return;
    };
    let line = format!("{} {} {args}\n", timestamp(), log.prefix);
    let _ = log.file.write_all(line.as_bytes());
}

/// Writes `<key>: <value>` only when the value differs from the last one.
pub fn transition(key: &str, value: String) {
    let Ok(mut last) = TRANSITIONS.lock() else {
        return;
    };
    if last.get(key) == Some(&value) {
        return;
    }
    write(format_args!("{key}: {value}"));
    last.insert(key.to_owned(), value);
}

#[must_use]
pub fn frame_summary(frame: &TerminalFrame) -> String {
    format!(
        "{}x{} cursor={},{} visible={} mouse={:?} alt={}",
        frame.rows.first().map_or(0, Vec::len),
        frame.rows.len(),
        frame.cursor.column,
        frame.cursor.row,
        frame.cursor.visible,
        frame.modes.mouse_tracking,
        frame.modes.alt_screen
    )
}

#[must_use]
pub fn client_summary(message: &ClientMsg) -> String {
    match message {
        ClientMsg::Hello { user_id, .. } => format!("Hello user={user_id}"),
        ClientMsg::Join { name, .. } => format!("Join name={name}"),
        ClientMsg::Invite { hours } => format!("Invite hours={hours:?}"),
        ClientMsg::DetachClient { client_id } => format!("DetachClient client={client_id}"),
        ClientMsg::ExitClient { pane } => format!("ExitClient pane={pane}"),
        ClientMsg::CreateTab { workspace } => format!("CreateTab workspace={workspace}"),
        ClientMsg::SplitPane { tab, direction, .. } => {
            format!("SplitPane tab={tab} direction={direction:?}")
        }
        ClientMsg::ClosePane { pane, .. } => format!("ClosePane pane={pane}"),
        ClientMsg::FocusPane { pane, .. } => format!("FocusPane pane={pane}"),
        ClientMsg::TerminalCapabilities { capabilities } => {
            format!(
                "TerminalCapabilities version={}",
                capabilities.protocol_version
            )
        }
        ClientMsg::TerminalInput { pane, input, .. } => {
            format!(
                "TerminalInput pane={pane} event={}",
                input_summary(&input.event)
            )
        }
        ClientMsg::Resize {
            tab, cols, rows, ..
        } => format!("Resize tab={tab} size={cols}x{rows}"),
        ClientMsg::GrantedInput {
            pane,
            bytes,
            sender,
            ..
        } => format!(
            "GrantedInput pane={pane} bytes={} sender={sender}",
            bytes.len()
        ),
        ClientMsg::PublishRuntime {
            user_id,
            generation,
            ..
        } => format!("PublishRuntime user={user_id} generation={generation}"),
        ClientMsg::RuntimeStream { user_id, .. } => format!("RuntimeStream user={user_id}"),
        ClientMsg::QueryTargets { user } => format!("QueryTargets user={user}"),
        ClientMsg::Terminals { user } => format!("Terminals user={user}"),
        ClientMsg::Watch {
            user,
            pane,
            cols,
            rows,
            viewer,
        } => format!("Watch user={user} pane={pane} size={cols}x{rows} viewer={viewer}"),
        ClientMsg::Unwatch { user, pane } => format!("Unwatch user={user} pane={pane}"),
        ClientMsg::TypeInto { user, pane, bytes } => {
            format!("TypeInto user={user} pane={pane} bytes={}", bytes.len())
        }
        ClientMsg::Stop => "Stop".to_owned(),
        ClientMsg::Leave => "Leave".to_owned(),
        ClientMsg::SetAllGrants { can_type } => format!("SetAllGrants can_type={can_type}"),
        ClientMsg::MouseInto { user, pane, mouse } => {
            format!("MouseInto user={user} pane={pane} kind={:?}", mouse.kind)
        }
        ClientMsg::GrantedMouse {
            pane,
            sender,
            mouse,
            ..
        } => format!(
            "GrantedMouse pane={pane} sender={sender} kind={:?}",
            mouse.kind
        ),
        ClientMsg::SetGrant { user, can_type } => {
            format!("SetGrant user={user} can_type={can_type}")
        }
        ClientMsg::ListPeople => "ListPeople".to_owned(),
        ClientMsg::QueryStatus => "QueryStatus".to_owned(),
        ClientMsg::AttachRuntime => "AttachRuntime".to_owned(),
        ClientMsg::ObserveRuntime => "ObserveRuntime".to_owned(),
        ClientMsg::Detach => "Detach".to_owned(),
    }
}

#[must_use]
pub fn server_summary(message: &ServerMsg) -> String {
    match message {
        ServerMsg::GrantsUpdated => "GrantsUpdated".to_owned(),
        ServerMsg::Terminals { user, terminals } => {
            let mut summary = format!("Terminals user={user}");
            for terminal in terminals {
                let _ = write!(
                    summary,
                    " {}={}x{}:{}",
                    terminal.pane, terminal.cols, terminal.rows, terminal.state
                );
            }
            summary
        }
        ServerMsg::Presence {
            user,
            online,
            idle_secs,
        } => format!("Presence user={user} online={online} idle={idle_secs}"),
        ServerMsg::Grants {
            can_type_here,
            you_may_type_into,
        } => format!(
            "Grants can_type_here={} you_may_type_into={}",
            can_type_here.join(","),
            you_may_type_into.join(",")
        ),
        ServerMsg::RuntimeReady { generation } => format!("RuntimeReady generation={generation}"),
        ServerMsg::Published { generation } => format!("Published generation={generation}"),
        ServerMsg::Welcome {
            user_id, client_id, ..
        } => format!("Welcome user={user_id} client={client_id}"),
        ServerMsg::Joined { user_id, name, .. } => format!("Joined user={user_id} name={name}"),
        ServerMsg::Seat {
            expires_in_secs, ..
        } => format!("Seat expires={expires_in_secs}"),
        ServerMsg::People { people } => format!("People count={}", people.len()),
        ServerMsg::Status {
            tabs,
            foreground,
            idle_secs,
        } => format!("Status tabs={tabs} foreground={foreground} idle={idle_secs}"),
        ServerMsg::Clients { clients } => format!("Clients count={}", clients.len()),
        ServerMsg::Targets { targets } => format!("Targets count={}", targets.len()),
        ServerMsg::Refused { reason } => format!("Refused reason={reason}"),
        ServerMsg::Bye { reason } => format!("Bye reason={reason}"),
        ServerMsg::Tree { tree } => {
            let tabs: usize = tree.workspaces.iter().map(|w| w.tabs.len()).sum();
            let panes: usize = tree
                .workspaces
                .iter()
                .flat_map(|w| &w.tabs)
                .map(|t| t.panes.len())
                .sum();
            format!("Tree tabs={tabs} panes={panes}")
        }
        ServerMsg::Frame { pane, bytes } => format!("Frame pane={pane} bytes={}", bytes.len()),
        ServerMsg::Cells { user, pane, frame } => {
            format!("Cells user={user} pane={pane} {}", frame_summary(frame))
        }
        ServerMsg::OpenStream { .. } => "OpenStream".to_owned(),
    }
}

fn input_summary(event: &InputEvent) -> String {
    match event {
        InputEvent::Key(key) => format!("Key({:?})", key.code),
        InputEvent::Text(text) => format!("Text(len={})", text.len()),
        InputEvent::Paste(text) => format!("Paste(len={})", text.len()),
        InputEvent::Mouse(mouse) => {
            format!("Mouse({:?},{},{})", mouse.kind, mouse.column, mouse.row)
        }
        InputEvent::Focus(gained) => format!("Focus({gained})"),
        InputEvent::Scrollback { lines } => format!("Scrollback({lines})"),
    }
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs();
    let (hours, minutes, secs) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    let (year, month, day) = civil_date(seconds / 86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{secs:02}.{:03}Z",
        now.subsec_millis()
    )
}

// Days since 1970-01-01 to a civil date, from Howard Hinnant's date algorithms.
fn civil_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}
