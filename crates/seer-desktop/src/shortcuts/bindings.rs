use super::*;
use gpui::Global;
use seer::Settings;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Bindings(BTreeMap<String, Chord>);
impl Global for Bindings {}

impl Action {
    fn id(self) -> &'static str {
        match self {
            Self::NewTerminal => "new_terminal",
            Self::ToggleSidebar => "toggle_sidebar",
            Self::FocusSidebar => "focus_sidebar",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::ScrollTop => "scroll_top",
            Self::ScrollBottom => "scroll_bottom",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::Rename => "rename",
            Self::TabMenu => "tab_menu",
        }
    }
}

pub(crate) fn saved_id(action: Action) -> String {
    let platform = if Platform::current() == Platform::Mac {
        "mac"
    } else {
        "linux"
    };
    format!("{platform}.{}", action.id())
}

impl Bindings {
    pub(crate) fn load(settings: &Settings) -> Result<Self, String> {
        Self::checked(settings, None)
    }

    fn checked(settings: &Settings, edited: Option<&Shortcut>) -> Result<Self, String> {
        let mut bindings = Self::default();
        for shortcut in SHORTCUTS {
            if let Some(text) = settings.shortcut_overrides.get(&saved_id(shortcut.action)) {
                let key = Keystroke::parse(text).map_err(|_| {
                    format!(
                        "Could not read keys for {}. Reset this shortcut.",
                        shortcut.name
                    )
                })?;
                let chord = captured(&key)?;
                bindings.0.insert(shortcut.action.id().to_owned(), chord);
            }
        }
        if let Some(shortcut) = edited {
            bindings.conflicts(shortcut.action, bindings.chords(shortcut))?;
        }
        for shortcut in SHORTCUTS {
            bindings.conflicts(shortcut.action, bindings.chords(shortcut))?;
        }
        Ok(bindings)
    }

    fn chords<'a>(&'a self, shortcut: &'a Shortcut) -> &'a [Chord] {
        self.0.get(shortcut.action.id()).map_or_else(
            || shortcut.chords(Platform::current()),
            std::slice::from_ref,
        )
    }

    fn conflicts(&self, action: Action, candidates: &[Chord]) -> Result<(), String> {
        for shortcut in SHORTCUTS
            .iter()
            .filter(|shortcut| shortcut.action != action)
        {
            if candidates.iter().any(|candidate| {
                self.chords(shortcut).iter().any(|other| {
                    candidate.key == other.key
                        && (candidate.required ^ other.required)
                            & !(candidate.ignored | other.ignored)
                            == 0
                })
            }) {
                return Err(format!("Used by {}. Choose other keys.", shortcut.name));
            }
        }
        Ok(())
    }

    pub(crate) fn change(
        settings: &Settings,
        index: usize,
        key: Option<&Keystroke>,
    ) -> Result<(Settings, Self), String> {
        let mut settings = settings.clone();
        let shortcut = &SHORTCUTS[index];
        let id = saved_id(shortcut.action);
        if let Some(key) = key {
            captured(key)?;
            let mut key = key.clone();
            key.modifiers.function = false;
            settings.shortcut_overrides.insert(id, key.unparse());
        } else {
            settings.shortcut_overrides.remove(&id);
        }
        let bindings = Self::checked(&settings, Some(shortcut))?;
        Ok((settings, bindings))
    }
}

impl Shortcut {
    pub(super) fn effective<'a>(&'a self, platform: Platform, cx: &'a App) -> &'a [Chord] {
        if platform == Platform::current()
            && let Some(bindings) = cx.try_global::<Bindings>()
        {
            return bindings.chords(self);
        }
        self.chords(platform)
    }

    pub(crate) fn effective_keys(&self, platform: Platform, cx: &App) -> String {
        if platform == Platform::current() && self.changed(cx) {
            Self::format(self.effective(platform, cx), platform)
        } else {
            self.keys(platform)
        }
    }

    pub(crate) fn changed(&self, cx: &App) -> bool {
        cx.try_global::<Bindings>()
            .is_some_and(|bindings| bindings.0.contains_key(self.action.id()))
    }
}

fn captured(key: &Keystroke) -> Result<Chord, String> {
    let held = bits(key.modifiers) & !FN;
    let function = key
        .key
        .strip_prefix('f')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=20).contains(&number));
    let character =
        key.key.len() == 1 && key.key.is_ascii() && !key.key.as_bytes()[0].is_ascii_control();
    let named = matches!(
        key.key.as_str(),
        "home" | "end" | "pageup" | "pagedown" | "insert" | "up" | "down" | "left" | "right"
    );
    let reserved = matches!(
        key.key.as_str(),
        "escape" | "tab" | "enter" | "keypadenter" | "backspace" | "delete" | "space"
    ) || (!function && held == 0)
        || (character && held & (CTRL | ALT | CMD) == 0)
        || (character && held == CTRL);
    if reserved {
        return Err("Reserved for terminal input. Choose other keys.".into());
    }
    if !(character || named || function) {
        return Err("Choose a letter, symbol, navigation key, or F1 to F20.".into());
    }
    Ok(Chord {
        key: Cow::Owned(key.key.clone()),
        required: held,
        ignored: FN,
    })
}
