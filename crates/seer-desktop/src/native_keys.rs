use std::ptr::{NonNull, null_mut};

use block2::RcBlock;
use gpui::{AppContext, Context, Keystroke, Modifiers, Window};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};

use crate::window::SeerWindow;

pub(crate) struct NativeKeys(Retained<AnyObject>);

pub(crate) fn install(window: &Window, cx: &Context<SeerWindow>) -> Option<NativeKeys> {
    let view = cx.entity().downgrade();
    let handle = window.window_handle();
    let app = cx.to_async();
    let block = RcBlock::new(move |event: NonNull<NSEvent>| {
        // AppKit supplies a live event for the duration of this main-thread callback.
        let native = unsafe { event.as_ref() };
        let mut app = app.clone();
        let handled = app
            .update_window(handle, |_, window, cx| {
                view.update(cx, |view, cx| {
                    let option_as_alt = cx
                        .global::<crate::terminal_config::OptionAsAlt>()
                        .active(native.modifierFlags().bits())
                        && view.input.focus.is_focused(window)
                        && !view.input.composition.marked();
                    let Some(key) = key(native, option_as_alt) else {
                        return false;
                    };
                    if !window.is_window_active() || view.invite_open() {
                        return false;
                    }
                    if view.settings_open() {
                        return view.settings_key(&key, window, cx);
                    }
                    if view.join_open() {
                        return false;
                    }
                    if key.key == "space" && !key.modifiers.control && !option_as_alt {
                        return false;
                    }
                    if key.modifiers.platform && !command_key(&key, cx) {
                        return false;
                    }
                    if view.window_command_key(&key, window, cx)
                        || view.sidebar_key(&key, window, cx)
                        || view.room_key(&key, window, cx)
                    {
                        return true;
                    }
                    if crate::window::terminals::new_shortcut(&key, cx) {
                        view.new_terminal(window, cx);
                        return true;
                    }
                    if view.switch_terminal_key(&key, window, cx) {
                        return true;
                    }
                    if view.rename_key(&key, window, cx) {
                        return true;
                    }
                    if !view.input.focus.is_focused(window) || view.input.composition.marked() {
                        return false;
                    }
                    view.terminal_key_with_alt(&key, option_as_alt, cx);
                    true
                })
                .unwrap_or(false)
            })
            .unwrap_or(false);
        if handled { null_mut() } else { event.as_ptr() }
    });
    // The block returns the supplied live event or null; AppKit retains the copied block.
    unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) }
        .map(NativeKeys)
}

impl Drop for NativeKeys {
    fn drop(&mut self) {
        // This token came from addLocalMonitor, and the view drops it on the main thread.
        unsafe { NSEvent::removeMonitor(&self.0) };
    }
}

fn command_key(key: &Keystroke, cx: &gpui::App) -> bool {
    use crate::shortcuts::{self, Scope};
    [Scope::Window, Scope::Terminal, Scope::Tabs]
        .into_iter()
        .any(|scope| shortcuts::action(scope, key, cx).is_some())
        || shortcuts::released(key, cx)
}

fn key(event: &NSEvent, option_as_alt: bool) -> Option<Keystroke> {
    let flags = event.modifierFlags();
    let command = flags.contains(NSEventModifierFlags::Command);
    let shift = flags.contains(NSEventModifierFlags::Shift);
    let alt = flags.contains(NSEventModifierFlags::Option);
    let control = flags.contains(NSEventModifierFlags::Control);
    // GPUI 0.2.2 sends non-printing keys to AppKit first. AppKit consumes
    // F20 and Option+Shift+Tab as text. Carbon virtual key codes do not vary by layout.
    let key = if let Some((_, key)) = KEYS.iter().find(|(code, _)| *code == event.keyCode()) {
        (*key).to_owned()
    } else if option_as_alt {
        event
            .charactersByApplyingModifiers(
                flags
                    & !(NSEventModifierFlags::Option
                        | NSEventModifierFlags::Shift
                        | NSEventModifierFlags::Control
                        | NSEventModifierFlags::Command),
            )?
            .to_string()
            .to_lowercase()
    } else if control || command {
        event
            .charactersIgnoringModifiers()?
            .to_string()
            .to_lowercase()
    } else {
        return None;
    };
    Some(Keystroke {
        key,
        // Alacritty uses NSEvent.characters after its default key bindings.
        // GPUI omits this text when Control is held.
        key_char: if option_as_alt {
            event.charactersByApplyingModifiers(flags & !NSEventModifierFlags::Option)
        } else {
            event.characters()
        }
        .map(|text| text.to_string()),
        modifiers: Modifiers {
            shift,
            alt,
            platform: command,
            control,
            ..Modifiers::default()
        },
    })
}

const KEYS: &[(u16, &str)] = &[
    (0x24, "enter"),
    (0x30, "tab"),
    (0x31, "space"),
    (0x33, "backspace"),
    (0x35, "escape"),
    (0x4c, "keypadenter"),
    (0x72, "insert"),
    (0x73, "home"),
    (0x74, "pageup"),
    (0x75, "delete"),
    (0x77, "end"),
    (0x79, "pagedown"),
    (0x7b, "left"),
    (0x7c, "right"),
    (0x7d, "down"),
    (0x7e, "up"),
    (0x7a, "f1"),
    (0x78, "f2"),
    (0x63, "f3"),
    (0x76, "f4"),
    (0x60, "f5"),
    (0x61, "f6"),
    (0x62, "f7"),
    (0x64, "f8"),
    (0x65, "f9"),
    (0x6d, "f10"),
    (0x67, "f11"),
    (0x6f, "f12"),
    (0x69, "f13"),
    (0x6b, "f14"),
    (0x71, "f15"),
    (0x6a, "f16"),
    (0x40, "f17"),
    (0x4f, "f18"),
    (0x50, "f19"),
    (0x5a, "f20"),
];
