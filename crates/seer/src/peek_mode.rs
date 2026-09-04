use std::cell::{Cell, RefCell};
use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use seer_core::proto::{ClientMsg, Person, TerminalInfo};
use seer_net::Stream;

use crate::drawer::{Drawer, DrawerAction};
use crate::tui::send;

thread_local! {
    static VIEW_ONLY: Cell<bool> = const { Cell::new(false) };
    static PEEK_PERSON: RefCell<Option<String>> = const { RefCell::new(None) };
    static WATCH: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
    static NOTICE: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_view_only(view_only: bool) {
    VIEW_ONLY.set(view_only);
    if !view_only {
        PEEK_PERSON.set(None);
    }
}

pub(crate) fn set_peek_person(person: Option<&str>) {
    VIEW_ONLY.set(person.is_some());
    PEEK_PERSON.set(person.map(str::to_owned));
}

pub(crate) fn is_view_only() -> bool {
    VIEW_ONLY.get()
}

pub(crate) fn peek_person() -> Option<String> {
    PEEK_PERSON.with_borrow(Clone::clone)
}

pub(crate) fn handle_drawer_key(
    key: KeyEvent,
    stream: &mut impl Stream,
    drawer: &mut Drawer,
) -> io::Result<bool> {
    match drawer.handle_key(key) {
        DrawerAction::Handled => return Ok(true),
        DrawerAction::Peek(person) => {
            request(stream, drawer, person)?;
            return Ok(true);
        }
        DrawerAction::Ignored => {}
    }
    if key.code != KeyCode::Esc || peek_person().is_none() {
        return Ok(false);
    }
    stop(stream)?;
    Ok(true)
}

fn request(stream: &mut impl Stream, drawer: &mut Drawer, person: Person) -> io::Result<()> {
    let user = person.user_id.clone();
    drawer.set_pending_peek(person);
    send(stream, &ClientMsg::Terminals { user })
}

pub(crate) fn start(
    stream: &mut impl Stream,
    drawer: &mut Drawer,
    targets: &[TerminalInfo],
) -> io::Result<()> {
    let Some(person) = drawer.take_pending_peek() else {
        return Ok(());
    };
    let Some(target) = targets.first() else {
        show_notice(format!("no target to peek for {}", person.name));
        return Ok(());
    };
    WATCH.set(Some((person.user_id.clone(), target.pane.clone())));
    send(
        stream,
        &ClientMsg::Watch {
            user: person.user_id,
            pane: target.pane.clone(),
        },
    )?;
    set_peek_person(Some(&person.name));
    Ok(())
}

pub(crate) fn refuse(drawer: &mut Drawer, reason: String) {
    drawer.take_pending_peek();
    show_notice(reason);
}

fn stop(stream: &mut impl Stream) -> io::Result<()> {
    if let Some((user, pane)) = WATCH.take() {
        send(stream, &ClientMsg::Unwatch { user, pane })?;
    }
    set_peek_person(None);
    Ok(())
}

pub(crate) fn take_notice() -> Option<String> {
    NOTICE.with_borrow_mut(Option::take)
}

fn show_notice(reason: String) {
    NOTICE.with_borrow_mut(|notice| *notice = Some(reason));
}
