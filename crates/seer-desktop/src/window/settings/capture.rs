use super::*;

impl SeerWindow {
    pub(in crate::window) fn start_capture(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.appearance.capture = Some(index);
        self.appearance.shortcut_error = None;
        self.input.reset();
        window.focus(&self.appearance.shortcut_focus[index]);
        self.reveal_settings_focus(&self.appearance.shortcut_focus[index]);
        cx.notify();
    }

    pub(in crate::window) fn cancel_capture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.appearance.capture.take() {
            window.focus(&self.appearance.shortcut_focus[index]);
        }
        self.appearance.shortcut_error = None;
        cx.notify();
    }

    pub(in crate::window) fn change_shortcut(
        &mut self,
        index: usize,
        key: Option<&Keystroke>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match shortcuts::Bindings::change(&self.appearance.settings, index, key) {
            Ok((settings, bindings)) => {
                if self.appearance.save(settings) {
                    cx.set_global(bindings);
                    self.appearance.capture = None;
                    self.appearance.shortcut_error = None;
                    window.focus(&self.appearance.shortcut_focus[index]);
                } else {
                    self.appearance.shortcut_error = Some((
                        index,
                        "Could not save settings. Check the config folder and try again.".into(),
                    ));
                }
            }
            Err(error) => self.appearance.shortcut_error = Some((index, error)),
        }
        self.reveal_settings_focus(&self.appearance.shortcut_focus[index]);
        cx.notify();
    }

    pub(super) fn capture_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.appearance.capture else {
            return false;
        };
        if key.key == "escape" {
            self.cancel_capture(window, cx);
        } else if !key.is_ime_in_progress() {
            self.change_shortcut(index, Some(key), window, cx);
        }
        true
    }

    pub(super) fn shortcut_activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .appearance
            .shortcut_focus
            .iter()
            .position(|focus| focus.is_focused(window))
        {
            self.start_capture(index, window, cx);
        } else if let Some(index) = self
            .appearance
            .shortcut_reset
            .iter()
            .position(|focus| focus.is_focused(window))
        {
            self.change_shortcut(index, None, window, cx);
        }
    }
}
