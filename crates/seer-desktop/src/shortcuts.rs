use gpui::{Keystroke, Modifiers};

mod defaults;
pub(crate) use defaults::SHORTCUTS;

const SHIFT: u8 = 1;
const CTRL: u8 = 2;
const ALT: u8 = 4;
const CMD: u8 = 8;
const FN: u8 = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Scope {
    Window,
    Terminal,
    Tabs,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Action {
    NewTerminal,
    ToggleSidebar,
    FocusSidebar,
    Copy,
    Paste,
    ScrollTop,
    ScrollBottom,
    PageUp,
    PageDown,
    Rename,
    TabMenu,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    Mac,
    Linux,
}

impl Platform {
    pub(crate) fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}

pub(crate) struct Shortcut {
    pub(crate) action: Action,
    pub(crate) scopes: &'static [Scope],
    pub(crate) context: &'static str,
    pub(crate) name: &'static str,
    mac: &'static [Chord],
    linux: &'static [Chord],
}

struct Chord {
    key: &'static str,
    required: u8,
    ignored: u8,
}

const fn chord(key: &'static str, required: u8, ignored: u8) -> Chord {
    Chord {
        key,
        required,
        ignored,
    }
}

fn bits(held: Modifiers) -> u8 {
    (u8::from(held.shift) * SHIFT)
        | (u8::from(held.control) * CTRL)
        | (u8::from(held.alt) * ALT)
        | (u8::from(held.platform) * CMD)
        | (u8::from(held.function) * FN)
}

impl Shortcut {
    fn chords(&self, platform: Platform) -> &'static [Chord] {
        match platform {
            Platform::Mac => self.mac,
            Platform::Linux => self.linux,
        }
    }

    pub(crate) fn keys(&self, platform: Platform) -> String {
        self.chords(platform)
            .iter()
            .map(|chord| chord.label(platform))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

impl Chord {
    fn matches(&self, key: &Keystroke) -> bool {
        self.key == key.key && bits(key.modifiers) & !self.ignored == self.required
    }

    fn label(&self, platform: Platform) -> String {
        let mut parts = Vec::new();
        for (flag, name) in [
            (CTRL, "Ctrl"),
            (ALT, "Alt"),
            (
                CMD,
                if platform == Platform::Mac {
                    "Cmd"
                } else {
                    "Super"
                },
            ),
            (SHIFT, "Shift"),
        ] {
            if self.required & flag != 0 {
                parts.push(name.to_owned());
            }
        }
        parts.push(match self.key {
            "pageup" => "Page Up".to_owned(),
            "pagedown" => "Page Down".to_owned(),
            key => {
                let mut chars = key.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                })
            }
        });
        parts.join(" ")
    }
}

pub(crate) fn action(scope: Scope, key: &Keystroke) -> Option<Action> {
    SHORTCUTS
        .iter()
        .find(|shortcut| {
            shortcut.scopes.contains(&scope)
                && shortcut
                    .chords(Platform::current())
                    .iter()
                    .any(|chord| chord.matches(key))
        })
        .map(|shortcut| shortcut.action)
}

pub(crate) fn keys(scope: Scope, action: Action) -> String {
    SHORTCUTS
        .iter()
        .filter(|shortcut| shortcut.scopes.contains(&scope) && shortcut.action == action)
        .map(|shortcut| shortcut.keys(Platform::current()))
        .collect::<Vec<_>>()
        .join(" / ")
}

#[cfg(target_os = "macos")]
pub(crate) fn native_command(code: u16, modifiers: Modifiers) -> Option<Keystroke> {
    // Carbon virtual key codes are independent of the active keyboard layout.
    let key = match code {
        0x0b => "b",
        0x2d => "n",
        _ => return None,
    };
    let key = Keystroke {
        key: key.to_owned(),
        key_char: None,
        modifiers,
    };
    action(Scope::Window, &key).map(|_| key)
}

pub(crate) enum Row {
    Heading(&'static str),
    Binding(usize),
}

pub(crate) fn rows() -> Vec<Row> {
    let mut contexts = Vec::new();
    for shortcut in SHORTCUTS {
        if !contexts.contains(&shortcut.context) {
            contexts.push(shortcut.context);
        }
    }
    contexts
        .into_iter()
        .flat_map(|context| {
            std::iter::once(Row::Heading(context)).chain(
                SHORTCUTS
                    .iter()
                    .enumerate()
                    .filter(move |(_, shortcut)| shortcut.context == context)
                    .map(|(index, _)| Row::Binding(index)),
            )
        })
        .collect()
}
