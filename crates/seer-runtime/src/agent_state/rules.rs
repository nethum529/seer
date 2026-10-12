use seer_core::proto::AgentState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub(super) fn classify(self, screen: &str, title: &str) -> Option<AgentState> {
        let lines: Vec<_> = screen
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let prompt = lines.iter().rposition(|line| self.ready_prompt(line));
        let controls = &lines[prompt.unwrap_or(0)..];
        if waiting(controls) || title.contains("Action Required") {
            return Some(AgentState::Waiting);
        }
        let before_prompt = &lines[..prompt.unwrap_or(lines.len())];
        if self.working(before_prompt) || busy_title(title) {
            return Some(AgentState::Working);
        }
        if prompt.is_some() {
            return Some(AgentState::Idle);
        }
        None
    }

    fn ready_prompt(self, line: &str) -> bool {
        let marker = match self {
            Self::Claude => '\u{276f}',
            Self::Codex => '\u{203a}',
        };
        let Some(body) = line.strip_prefix(marker) else {
            return false;
        };
        !body.trim_start().starts_with(|c: char| c.is_ascii_digit())
    }

    fn working(self, lines: &[&str]) -> bool {
        let tail = &lines[lines.len().saturating_sub(6)..];
        match self {
            Self::Claude => claude_working(lines),
            Self::Codex => tail
                .iter()
                .rposition(|line| line.contains("to interrupt)"))
                .is_some_and(|at| {
                    !tail[at + 1..].iter().any(|line| {
                        line.starts_with(['\u{2022}', '\u{25a0}', '\u{2713}', '\u{2717}'])
                            || line.starts_with("Worked for ")
                    })
                }),
        }
    }
}

fn claude_working(lines: &[&str]) -> bool {
    let Some(at) = lines.iter().rposition(|line| {
        line.starts_with([
            '*', '\u{00b7}', '\u{2722}', '\u{2733}', '\u{2736}', '\u{273b}', '\u{273d}',
            '\u{23f8}', '\u{23f5}',
        ])
    }) else {
        return false;
    };
    let line = lines[at];
    let interrupt = line.contains("esc to interrupt")
        || line.contains("background agents to finish")
        || line.contains("MCP tasks still running");
    let body = line.chars().next().map(|c| &line[c.len_utf8()..]);
    let short = body.is_some_and(|body| {
        body.starts_with(' ')
            && body.trim_start().starts_with(char::is_alphabetic)
            && (body.ends_with('\u{2026}') || body.ends_with("..."))
    });
    (interrupt || (short && short_footer(&lines[at + 1..])))
        && !lines[at + 1..].iter().any(|line| {
            line.starts_with(['\u{23fa}', '\u{276f}'])
                || line
                    .strip_prefix('\u{23bf}')
                    .is_some_and(|body| !body.trim_start().starts_with("Tip:"))
        })
}

fn short_footer(lines: &[&str]) -> bool {
    let mut tip = false;
    lines.iter().all(|line| {
        if line.chars().all(|c| c == '\u{2500}') {
            tip = false;
            return true;
        }
        if let Some(body) = line.strip_prefix('\u{23bf}') {
            tip = body.trim_start().starts_with("Tip:");
            return tip;
        }
        tip && !line.starts_with(['`', '~', '>'])
    })
}

fn waiting(lines: &[&str]) -> bool {
    let footer = lines[lines.len().saturating_sub(8)..]
        .join(" ")
        .to_ascii_lowercase();
    let cancel = footer.contains("esc to cancel");
    (cancel && (footer.contains("enter to confirm") || footer.contains("enter to select")))
        || footer.contains("enter to submit answer")
        || footer.contains("enter to submit all")
        || (footer.contains("trust this folder?") && footer.contains("trust and continue"))
        || (footer.contains("do you want to proceed?") && footer.contains("yes"))
}

fn busy_title(title: &str) -> bool {
    title
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '\u{2800}'..='\u{28ff}' | '\u{25d0}'..='\u{25d3}'))
}
