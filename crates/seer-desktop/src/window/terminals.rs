use super::*;

pub(crate) fn new_shortcut(key: &Keystroke) -> bool {
    let held = key.modifiers;
    key.key == "n"
        && !held.alt
        && !held.function
        && if cfg!(target_os = "macos") {
            held.platform && !held.control && !held.shift
        } else {
            held.control && held.shift && !held.platform
        }
}

pub(crate) fn new_shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd N"
    } else {
        "Ctrl Shift N"
    }
}

impl SeerWindow {
    pub(super) fn new_terminal_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.editing.is_some() || self.menu.is_some() {
            return false;
        }
        if !new_shortcut(key)
            && !(self.keys.new_terminal.is_focused(window)
                && matches!(key.key.as_str(), "enter" | "space"))
        {
            return false;
        }
        self.new_terminal(window, cx);
        true
    }

    pub(crate) fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.people_menu = None;
        self.input.reset();
        self.rows = Rows::default();
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.new_terminal(),
            Content::Message(_) | Content::Join(_) => return,
        };
        self.check(result, cx);
        window.focus(&self.input.focus);
        self.notify_room(cx);
    }

    pub(super) fn close_terminal(
        &mut self,
        pane: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.room.modal() || self.invite_open() {
            return;
        }
        let Content::Terminal(link, _) = &self.content else {
            return;
        };
        if link.remote() {
            return;
        }
        let Some(terminal) = link
            .terminals()
            .iter()
            .find(|terminal| terminal.pane == pane)
        else {
            return;
        };
        let pane = terminal.pane.clone();
        let inspection = link.inspect_terminal(pane.clone());
        self.menu = None;
        self.input.reset();
        let inspected = cx.background_executor().spawn(inspection);
        cx.spawn_in(window, async move |view, cx| {
            let result = inspected.await;
            let _ = view.update_in(cx, |view, window, cx| match result {
                Ok(program) => view.finish_close(&pane, &program, window, cx),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => view.check(Err(error), cx),
            });
        })
        .detach();
        self.notify_room(cx);
    }

    fn finish_close(
        &mut self,
        pane: &str,
        program: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.room.modal() || self.invite_open() {
            return;
        }
        let Content::Terminal(link, _) = &self.content else {
            return;
        };
        if link.remote()
            || !link
                .terminals()
                .iter()
                .any(|terminal| terminal.pane == pane)
        {
            return;
        }
        if program == "shell" {
            self.send_close(pane, cx);
            window.focus(&self.keys.tabs);
        } else {
            self.confirm_terminal(pane, program, window, cx);
        }
        self.notify_room(cx);
    }

    pub(super) fn send_close(&mut self, pane: &str, cx: &mut Context<Self>) {
        let result = match &mut self.content {
            Content::Terminal(link, _) => link.close_terminal(pane),
            Content::Message(_) | Content::Join(_) => Ok(()),
        };
        self.check(result, cx);
    }
}
