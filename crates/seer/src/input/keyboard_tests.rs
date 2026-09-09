use super::tests::{one_terminal_state, quiet};
use super::*;
use seer_core::proto::{ClientMsg, codec};
use std::os::unix::net::UnixStream;

#[test]
fn typing_from_the_overview_sends_the_first_key_instead_of_running_shortcuts() {
    let (mut state, pane) = one_terminal_state();
    let (mut stream, mut peer) = UnixStream::pair().expect("streams");
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout");
    let keys = "qxncpsmhjkl/123456789"
        .chars()
        .map(|character| KeyEvent::new(CrosstermKeyCode::Char(character), KeyModifiers::NONE))
        .chain([
            KeyEvent::new(CrosstermKeyCode::Enter, KeyModifiers::NONE),
            KeyEvent::new(CrosstermKeyCode::Esc, KeyModifiers::NONE),
            KeyEvent::new(CrosstermKeyCode::Char('b'), KeyModifiers::CONTROL),
        ])
        .collect::<Vec<_>>();

    for pinned in [false, true] {
        state.chrome.pinned = pinned;
        for key in &keys {
            state.viewer = None;
            assert!(
                !command(*key, &mut stream, &mut state).expect("typing"),
                "{key:?} must not quit Seer"
            );
            let message: ClientMsg = codec::decode(&mut peer).expect("first key must reach PTY");
            let ClientMsg::TerminalInput {
                pane: target,
                input,
                ..
            } = message
            else {
                panic!("typing must send terminal input, got {message:?}");
            };
            assert_eq!(target, pane);
            assert_eq!(input, key_to_input(*key).expect("terminal key"));
            assert!(
                !state.chrome_owns_input(),
                "typing must not open Seer controls"
            );
        }
    }
}

#[test]
fn typing_from_the_overview_respects_remote_grants() {
    let (mut state, pane) = one_terminal_state();
    let (mut stream, mut peer) = UnixStream::pair().expect("streams");
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout");
    let mut bob = state.people[0].clone();
    bob.user_id = "bob".into();
    bob.name = "Bob".into();
    state.people.push(bob);
    state
        .terminals
        .insert("bob".into(), state.terminals["alice"].clone());
    state.select_person(1);
    let key = KeyEvent::new(CrosstermKeyCode::Char('q'), KeyModifiers::NONE);
    assert!(!command(key, &mut stream, &mut state).expect("read-only typing"));
    assert!(quiet(&mut peer), "read-only input must stay off the wire");

    state.you_may_type_into.insert("bob".into());
    state.viewer = None;
    assert!(!command(key, &mut stream, &mut state).expect("granted typing"));
    assert_eq!(
        codec::decode::<_, ClientMsg>(&mut peer).expect("granted input"),
        ClientMsg::TypeInto {
            user: "bob".into(),
            pane,
            bytes: b"q".to_vec(),
        }
    );
    state.you_may_type_into.clear();
    state.viewer = None;
    assert!(!command(key, &mut stream, &mut state).expect("revoked typing"));
    assert!(quiet(&mut peer), "revoked input must stay off the wire");
}

#[test]
fn pasting_from_the_overview_reaches_the_selected_terminal() {
    let (mut state, pane) = one_terminal_state();
    let (mut stream, mut peer) = UnixStream::pair().expect("streams");
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout");
    let input = TerminalInput::new(InputEvent::Paste("query".into()));
    crate::viewer::input_message(&mut stream, &mut state, input.clone()).expect("paste");
    assert!(matches!(
        codec::decode::<_, ClientMsg>(&mut peer).expect("paste must reach PTY"),
        ClientMsg::TerminalInput { pane: target, input: sent, .. }
            if target == pane && sent == input
    ));
}

#[test]
fn an_empty_overview_has_no_keyboard_actions() {
    let (mut state, _) = one_terminal_state();
    state.terminals.clear();
    let (mut stream, mut peer) = UnixStream::pair().expect("streams");
    peer.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("timeout");
    for character in "qxncpsm/".chars() {
        assert!(
            !command(
                KeyEvent::new(CrosstermKeyCode::Char(character), KeyModifiers::NONE),
                &mut stream,
                &mut state
            )
            .expect("key")
        );
        assert!(!state.chrome_owns_input());
    }
    assert!(
        quiet(&mut peer),
        "letters must not create or close terminals"
    );
}
