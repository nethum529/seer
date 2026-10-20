use super::*;
use gpui::{App, FocusHandle};
use seer::processes::{Kind, Process};

mod paint;

pub(crate) struct Processes {
    pub(in crate::window) rows: Vec<Row>,
    pub(in crate::window) refresh: FocusHandle,
    pub(in crate::window) clean: FocusHandle,
    pub(in crate::window) busy: bool,
    pub(in crate::window) error: Option<String>,
    pub(in crate::window) sheet: Option<StopSheet>,
    focus_refresh: bool,
}

pub(in crate::window) struct Row {
    pub(in crate::window) process: Process,
    pub(in crate::window) focus: FocusHandle,
}

impl Row {
    pub(in crate::window) fn can_stop(&self) -> bool {
        self.process.kind() == Kind::Runtime && !self.process.used_by(std::process::id())
    }
}

pub(in crate::window) struct StopSheet {
    pid: u32,
    pub(in crate::window) cancel: FocusHandle,
    confirm: FocusHandle,
    return_focus: FocusHandle,
    motion: Motion,
}

#[derive(Clone, Copy)]
pub(super) enum ProcessAction {
    Clean,
    Stop(u32),
}

impl Processes {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            rows: Vec::new(),
            refresh: cx.focus_handle().tab_stop(true),
            clean: cx.focus_handle().tab_stop(true),
            busy: false,
            error: None,
            sheet: None,
            focus_refresh: false,
        }
    }

    pub(in crate::window) fn can_clean(&self) -> bool {
        self.rows.iter().any(|row| row.process.can_clean())
    }
}

impl SeerWindow {
    pub(in crate::window::settings) fn load_processes(
        &mut self,
        action: Option<ProcessAction>,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.appearance.processes;
        if state.busy {
            return;
        }
        state.busy = true;
        state.error = None;
        let work = cx.background_executor().spawn(async move {
            let action = match action {
                Some(ProcessAction::Clean) => seer::processes::clean()
                    .map(|_| ())
                    .map_err(|_| "Could not clean up the windows. Try again.".to_owned()),
                Some(ProcessAction::Stop(pid)) => seer::processes::stop(pid)
                    .map(|_| ())
                    .map_err(|_| "Could not stop this runtime. Try again.".to_owned()),
                None => Ok(()),
            };
            (action, seer::processes::list())
        });
        cx.spawn(async move |view, cx| {
            let (result, listed) = work.await;
            let _ = view.update(cx, |view, cx| {
                let state = &mut view.appearance.processes;
                state.busy = false;
                state.error = result.as_ref().err().cloned();
                if let Ok(processes) = listed {
                    let previous = std::mem::take(&mut state.rows);
                    state.rows = processes
                        .into_iter()
                        .map(|process| {
                            let focus = previous
                                .iter()
                                .find(|row| row.process.pid() == process.pid())
                                .map_or_else(
                                    || cx.focus_handle().tab_stop(true),
                                    |row| row.focus.clone(),
                                );
                            Row { process, focus }
                        })
                        .collect();
                } else {
                    state.error.get_or_insert_with(|| {
                        "Could not read the processes. Try Refresh again.".to_owned()
                    });
                }
                if result.is_ok()
                    && let Some(sheet) = &mut state.sheet
                {
                    sheet.motion.close(view.keys.mode == Mode::Keyboard, 140);
                    state.focus_refresh = true;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::window::settings) fn process_activation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &self.appearance.processes;
        if state.refresh.is_focused(window) {
            self.load_processes(None, cx);
        } else if state.clean.is_focused(window) && state.can_clean() {
            window.focus(&state.refresh);
            self.load_processes(Some(ProcessAction::Clean), cx);
        } else if let Some(row) = state
            .rows
            .iter()
            .find(|row| row.can_stop() && row.focus.is_focused(window))
        {
            self.confirm_process(row.process.pid(), window, cx);
        }
    }

    fn confirm_process(&mut self, pid: u32, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.appearance.processes;
        if state.busy || state.sheet.is_some() {
            return;
        }
        let Some(row) = state
            .rows
            .iter()
            .find(|row| row.process.pid() == pid && row.can_stop())
        else {
            return;
        };
        let cancel = cx.focus_handle();
        window.focus(&cancel);
        state.error = None;
        state.sheet = Some(StopSheet {
            pid,
            cancel,
            confirm: cx.focus_handle(),
            return_focus: row.focus.clone(),
            motion: Motion::open(self.keys.mode == Mode::Keyboard, 200),
        });
        cx.notify();
    }

    fn close_process_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.appearance.processes;
        if state.busy {
            return;
        }
        if let Some(sheet) = &mut state.sheet {
            sheet.motion.close(self.keys.mode == Mode::Keyboard, 140);
            window.focus(&sheet.return_focus);
        }
        cx.notify();
    }

    fn stop_process(&mut self, cx: &mut Context<Self>) {
        if let Some(sheet) = &self.appearance.processes.sheet {
            self.load_processes(Some(ProcessAction::Stop(sheet.pid)), cx);
        }
    }

    pub(in crate::window::settings) fn process_sheet_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(sheet) = &self.appearance.processes.sheet else {
            return false;
        };
        if sheet.motion.closing() {
            return true;
        }
        match key.key.as_str() {
            "escape" => self.close_process_sheet(window, cx),
            "tab" => window.focus(if sheet.cancel.is_focused(window) {
                &sheet.confirm
            } else {
                &sheet.cancel
            }),
            "enter" | "space" if !self.appearance.processes.busy => {
                if sheet.confirm.is_focused(window) {
                    self.stop_process(cx);
                } else {
                    self.close_process_sheet(window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }
}
