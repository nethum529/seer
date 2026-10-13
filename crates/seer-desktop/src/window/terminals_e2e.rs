use gpui::{TestAppContext, VisualTestContext};

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
