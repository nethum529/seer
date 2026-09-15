use seer_core::{InputEvent, KeyCode, KeyInput, Modifiers};
use std::io;
use std::time::Duration;

pub(crate) enum Step {
    Event(InputEvent),
    Wait(Duration),
}

// A script is plain text. A newline is Enter. A backslash starts one of:
// \e Escape, \t Tab, \\ a backslash, \w a one second pause with no key.
pub(crate) fn parse(text: &str) -> io::Result<Vec<Step>> {
    let mut steps = Vec::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        let step = match character {
            '\n' => key(KeyCode::Enter),
            '\\' => escaped(characters.next())?,
            other => Step::Event(InputEvent::Text(other.to_string())),
        };
        steps.push(step);
    }
    Ok(steps)
}

fn escaped(code: Option<char>) -> io::Result<Step> {
    Ok(match code {
        Some('e') => key(KeyCode::Escape),
        Some('t') => key(KeyCode::Tab),
        Some('\\') => Step::Event(InputEvent::Text("\\".into())),
        Some('w') => Step::Wait(Duration::from_secs(1)),
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown script escape: {other:?}"),
            ));
        }
    })
}

fn key(code: KeyCode) -> Step {
    Step::Event(InputEvent::Key(KeyInput {
        code,
        modifiers: Modifiers::default(),
    }))
}

pub(crate) fn key_count(steps: &[Step]) -> usize {
    steps
        .iter()
        .filter(|step| matches!(step, Step::Event(_)))
        .count()
}
