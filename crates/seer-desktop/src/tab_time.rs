use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Context, Task};
use seer_core::proto::TerminalInfo;

use crate::window::{Content, SeerWindow};

pub(crate) fn label(terminal: &TerminalInfo) -> Option<String> {
    terminal.agent_state?;
    let since = UNIX_EPOCH.checked_add(Duration::from_millis(terminal.agent_state_since?))?;
    let seconds = SystemTime::now()
        .duration_since(since)
        .unwrap_or_default()
        .as_secs();
    Some(match seconds {
        0..60 => "now".into(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    })
}

pub(crate) fn refresh(cx: &mut Context<SeerWindow>) -> Task<()> {
    cx.spawn(async move |view, cx| {
        let mut previous = Vec::new();
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if view
                .update(cx, |view, cx| {
                    let current: Vec<_> = match &view.content {
                        Content::Terminal(link, _) => link.terminals().iter().map(label).collect(),
                        Content::Message(_) | Content::Join(_) => Vec::new(),
                    };
                    if current != previous {
                        previous = current;
                        cx.notify();
                        #[cfg(target_os = "macos")]
                        if let Some(frame) = &view.input.frame {
                            frame.request(cx);
                        }
                    }
                })
                .is_err()
            {
                break;
            }
        }
    })
}
