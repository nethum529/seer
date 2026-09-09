use crate::{state::ClientState, tui::send};
use seer_core::proto::ClientMsg;
use std::{cell::RefCell, io};

thread_local! { static START_PERSON: RefCell<Option<String>> = const { RefCell::new(None) }; }
pub(crate) fn set_peek_person(person: Option<&str>) {
    START_PERSON.set(person.map(str::to_owned));
}
pub(crate) fn take_start_person() -> Option<String> {
    START_PERSON.take()
}

pub(crate) fn new_terminal(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<()> {
    let Some(workspace) = state.tree.workspaces.first() else {
        state.set_notice("Waiting for your terminals.");
        return Ok(());
    };
    state.pending_new = Some(
        state
            .tree
            .workspaces
            .iter()
            .flat_map(|w| &w.tabs)
            .flat_map(|t| &t.panes)
            .map(|p| p.id.clone())
            .collect(),
    );
    send(
        stream,
        &ClientMsg::CreateTab {
            workspace: workspace.id.clone(),
        },
    )?;
    Ok(())
}

pub(crate) fn invite(
    stream: &mut crate::routes::Routes,
    state: &mut ClientState,
) -> io::Result<()> {
    if !state.invite_pending {
        send(stream, &ClientMsg::Invite { hours: Some(24) })?;
        state.invite_pending = true;
    }
    Ok(())
}

pub(crate) fn copy_invite(state: &mut ClientState) -> io::Result<()> {
    let Some(invite) = &state.invite else {
        return Ok(());
    };
    crate::input::copy_text(invite)?;
    state.set_notice("Copy requested. Your terminal must permit clipboard access.");
    Ok(())
}
