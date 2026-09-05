use std::{
    io::{self, IsTerminal, Write},
    thread,
    time::Duration,
};

pub(super) fn print(wordmark: &str) {
    if !io::stdout().is_terminal() {
        print!("{wordmark}");
        return;
    }
    for line in wordmark.lines() {
        println!("{line}");
        let _ = io::stdout().flush();
        thread::sleep(Duration::from_millis(100));
    }
}
