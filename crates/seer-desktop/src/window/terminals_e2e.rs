use gpui::{TestAppContext, VisualTestContext};
use seer_core::proto::{ClientMsg, ServerMsg, codec};

use super::input_e2e::{Runtime, open_view};
use super::watch_e2e::pump;
use super::*;

fn new_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-n"
    } else {
        "ctrl-shift-n"
    }
}

fn count(view: &Entity<SeerWindow>, cx: &VisualTestContext) -> usize {
    view.read_with(cx, |view, _| match &view.content {
        Content::Terminal(link, _) => link.terminals().len(),
        _ => panic!("the terminal connection must stay open"),
    })
}

#[gpui::test]
fn stale_close_keeps_the_remaining_terminal_and_connection(cx: &mut TestAppContext) {
    let runtime = Runtime::start_shell();
    let (view, cx) = open_view(&runtime, cx);
    pump(cx);
    cx.simulate_keystrokes(new_key());
    pump(cx);
    assert_eq!(count(&view, cx), 2);
    let pane = view.read_with(cx, |view, _| match &view.content {
        Content::Terminal(link, _) => link.terminals()[1].pane.clone(),
        _ => panic!("the terminal connection must stay open"),
    });
    let mut other =
        std::os::unix::net::UnixStream::connect(runtime.directory.path().join("socket")).unwrap();
    other
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let _: ServerMsg = codec::decode(&mut other).unwrap();
    codec::encode(&mut other, &ClientMsg::AttachRuntime).unwrap();
    let tree = loop {
        if let ServerMsg::Tree { tree } = codec::decode(&mut other).unwrap() {
            break tree;
        }
    };
    let workspace = &tree.workspaces[0];
    let tab = workspace
        .tabs
        .iter()
        .find(|tab| tab.panes.iter().any(|item| item.id == pane))
        .unwrap();
    codec::encode(
        &mut other,
        &ClientMsg::ClosePane {
            workspace: workspace.id.clone(),
            tab: tab.id.clone(),
            pane: pane.clone(),
        },
    )
    .unwrap();
    loop {
        if let ServerMsg::Tree { tree } = codec::decode(&mut other).unwrap()
            && !tree
                .workspaces
                .iter()
                .flat_map(|w| &w.tabs)
                .flat_map(|t| &t.panes)
                .any(|item| item.id == pane)
        {
            break;
        }
    }
    // The main window has not consumed the other client's Close event yet.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.close_terminal(&pane, window, cx));
    });
    pump(cx);
    assert_eq!(count(&view, cx), 1);
    assert!(cx.debug_bounds("room-confirm").is_none());
    cx.simulate_keystrokes("tab");
    cx.simulate_input("echo T452_STILL_LINKED");
    cx.simulate_keystrokes("enter");
    pump(cx);
    assert!(view.update(cx, |view, _| {
        view.with_raw(|grid| {
            (0..5).any(|row| {
                grid.row(row)
                    .iter()
                    .map(|cell| cell.character)
                    .collect::<String>()
                    .trim()
                    == "T452_STILL_LINKED"
            })
        })
        .unwrap_or(false)
    }));
    cx.simulate_keystrokes(new_key());
    pump(cx);
    assert_eq!(count(&view, cx), 2);
}

#[gpui::test]
fn close_shell_selects_next_and_last_close_keeps_new_terminal(cx: &mut TestAppContext) {
    let runtime = Runtime::start_shell();
    let (view, cx) = open_view(&runtime, cx);
    pump(cx);
    cx.simulate_keystrokes(new_key());
    pump(cx);
    assert_eq!(count(&view, cx), 2);
    cx.simulate_keystrokes("secondary-shift-b shift-f10 down enter");
    pump(cx);
    assert_eq!(count(&view, cx), 1);
    assert!(cx.debug_bounds("room-confirm").is_none());
    cx.simulate_keystrokes("shift-f10 down enter");
    pump(cx);
    assert_eq!(count(&view, cx), 0);
    let button = cx.debug_bounds("new-terminal").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    pump(cx);
    assert_eq!(count(&view, cx), 1);
}

#[gpui::test]
fn close_program_asks_cancel_keeps_it_and_confirm_closes_it(cx: &mut TestAppContext) {
    let runtime = Runtime::start_shell();
    let (view, cx) = open_view(&runtime, cx);
    pump(cx);
    cx.simulate_keystrokes("tab");
    cx.simulate_input("sleep 60");
    cx.simulate_keystrokes("enter");
    pump(cx);
    cx.simulate_keystrokes("secondary-shift-b shift-f10 down enter");
    pump(cx);
    assert!(cx.debug_bounds("room-confirm").is_some());
    assert_eq!(count(&view, cx), 1);
    cx.simulate_keystrokes("enter");
    pump(cx);
    assert_eq!(count(&view, cx), 1);
    cx.simulate_keystrokes("shift-f10 down enter");
    pump(cx);
    assert!(cx.debug_bounds("room-confirm").is_some());
    cx.simulate_keystrokes("tab enter");
    pump(cx);
    assert_eq!(count(&view, cx), 0);
}

#[gpui::test]
fn new_terminal_from_empty_keeps_immediate_typing(cx: &mut TestAppContext) {
    let runtime = Runtime::start_shell();
    let (view, cx) = open_view(&runtime, cx);
    pump(cx);
    cx.simulate_keystrokes("shift-f10 down enter");
    pump(cx);
    assert_eq!(count(&view, cx), 0);
    cx.simulate_keystrokes(new_key());
    cx.simulate_input("echo T452_TYPED");
    cx.simulate_keystrokes("enter");
    pump(cx);
    let echoed = view.update(cx, |view, _| {
        view.with_raw(|grid| {
            (0..5).any(|row| {
                grid.row(row)
                    .iter()
                    .map(|cell| cell.character)
                    .collect::<String>()
                    .trim()
                    == "T452_TYPED"
            })
        })
        .unwrap_or(false)
    });
    assert!(
        echoed,
        "typing after New terminal must reach the new shell in full"
    );
}

#[gpui::test]
fn close_after_starting_program_checks_the_live_process(cx: &mut TestAppContext) {
    let runtime = Runtime::start_shell();
    let (view, cx) = open_view(&runtime, cx);
    pump(cx);
    cx.simulate_keystrokes("tab");
    cx.simulate_input("sleep 60");
    cx.simulate_keystrokes("enter");
    std::thread::sleep(std::time::Duration::from_millis(60));
    cx.simulate_keystrokes("secondary-shift-b shift-f10 down enter");
    pump(cx);
    assert!(cx.debug_bounds("room-confirm").is_some());
    assert_eq!(count(&view, cx), 1);
}

#[gpui::test]
fn close_background_program_asks_and_cancel_keeps_the_job(cx: &mut TestAppContext) {
    let runtime = Runtime::start_shell();
    let (view, cx) = open_view(&runtime, cx);
    pump(cx);
    let pid_file = runtime.directory.path().join("background-pid");
    cx.simulate_keystrokes("tab");
    cx.simulate_input(&format!("sleep 600 & echo $! > {}", pid_file.display()));
    cx.simulate_keystrokes("enter");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !pid_file.exists() && std::time::Instant::now() < deadline {
        pump(cx);
    }
    let pid = std::fs::read_to_string(pid_file).unwrap();
    cx.simulate_keystrokes("secondary-shift-b shift-f10 down enter");
    pump(cx);
    assert!(cx.debug_bounds("room-confirm").is_some());
    assert_eq!(count(&view, cx), 1);
    cx.simulate_keystrokes("enter");
    pump(cx);
    assert!(cx.debug_bounds("room-confirm").is_none());
    assert_eq!(count(&view, cx), 1);
    assert!(
        std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .status()
            .unwrap()
            .success()
    );
    cx.simulate_keystrokes("shift-f10 down enter");
    pump(cx);
    cx.simulate_keystrokes("tab enter");
    pump(cx);
    assert_eq!(count(&view, cx), 0);
}
