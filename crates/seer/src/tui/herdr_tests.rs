use crossterm::event::KeyCode;
use seer_core::proto::ClientMsg;
use seer_core::{InputEvent, TerminalInput};

use super::set_view_only;
use super::test_support::{
    apply_own_foreground, assert_key_input, decode, send_prefixed_key, socket_pair, state_with_pane,
};
use crate::drawer::Drawer;

#[test]
fn herdr_in_front_forwards_the_prefix_key() {
    let (mut client, mut server) = socket_pair();
    let mut state = state_with_pane();
    let mut drawer = Drawer::default();
    let mut command_pending = false;
    set_view_only(false);

    apply_own_foreground(&mut client, &mut state, &mut drawer, "herdr");
    send_prefixed_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('c'),
        &mut drawer,
    );

    assert_eq!(
        decode(&mut server),
        ClientMsg::TerminalInput {
            workspace: "w1".into(),
            tab: "w1:t1".into(),
            pane: "w1:p1".into(),
            input: TerminalInput::new(InputEvent::Text("\u{2}".into())),
        }
    );
    assert_key_input(&mut server, "w1:p1", 'c');

    apply_own_foreground(&mut client, &mut state, &mut drawer, "fish");
    send_prefixed_key(
        &mut client,
        &mut state,
        &mut command_pending,
        KeyCode::Char('c'),
        &mut drawer,
    );

    assert_eq!(
        decode(&mut server),
        ClientMsg::CreateTab {
            workspace: "w1".into()
        }
    );
}
