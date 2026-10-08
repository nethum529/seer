// A sequence that never ends must not grow memory. A new window gets only
// the start of it, which puts its parser in the same state.
const KEEP: usize = 4096;
const ESC: u8 = 0x1b;

/// The output since the parser last left its ground state: an escape
/// sequence or a UTF-8 character that a later read completes. A new
/// window gets these bytes after the restore, so its parser continues
/// from the same state.
#[derive(Default)]
pub(super) struct Unfinished {
    state: State,
    bytes: Vec<u8>,
}

// The states of the vte 0.15 parser that alacritty_terminal uses. Only
// ESC leaves the ground state, and ESC enters the escape state from every
// state, so the bytes after the last ESC give the state.
#[derive(Clone, Copy, Default, PartialEq)]
enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    OscString,
    SosPmApcString,
}

impl Unfinished {
    pub(super) fn follow(&mut self, chunk: &[u8]) {
        let mut rest = chunk;
        if let Some(start) = memchr::memrchr(ESC, chunk) {
            self.state = State::Escape;
            self.bytes.clear();
            self.keep(&chunk[start..=start]);
            rest = &chunk[start + 1..];
        }
        if self.state != State::Ground {
            let Some(end) = self.follow_sequence(rest) else {
                return;
            };
            self.bytes.clear();
            rest = &rest[end + 1..];
        }
        self.bytes
            .extend_from_slice(&rest[rest.len().saturating_sub(3)..]);
        let open = open_utf8(&self.bytes);
        self.bytes.drain(..self.bytes.len() - open);
    }

    // Gives where the sequence ends. A control byte inside an escape or CSI
    // sequence runs at once and does not change the state, so a new window
    // must not get it again.
    fn follow_sequence(&mut self, bytes: &[u8]) -> Option<usize> {
        for (index, &byte) in bytes.iter().enumerate() {
            if runs_at_once(self.state, byte) {
                continue;
            }
            self.state = next(self.state, byte);
            if self.state == State::Ground {
                return Some(index);
            }
            self.keep(&[byte]);
        }
        None
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn keep(&mut self, bytes: &[u8]) {
        let room = KEEP.saturating_sub(self.bytes.len()).min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..room]);
    }
}

fn next(state: State, byte: u8) -> State {
    use State::*;
    match (state, byte) {
        (_, 0x18 | 0x1a) => Ground,
        (_, ESC) => Escape,
        (Escape, 0x20..=0x2f) => EscapeIntermediate,
        (Escape, 0x50) => DcsEntry,
        (Escape, 0x58 | 0x5e | 0x5f) => SosPmApcString,
        (Escape, 0x5b) => CsiEntry,
        (Escape, 0x5d) => OscString,
        (Escape | EscapeIntermediate, 0x30..=0x7e) => Ground,
        (CsiEntry | CsiParam, 0x20..=0x2f) => CsiIntermediate,
        (CsiEntry, 0x30..=0x3f) => CsiParam,
        (CsiParam, 0x3c..=0x3f) | (CsiIntermediate, 0x30..=0x3f) => CsiIgnore,
        (CsiEntry | CsiParam | CsiIntermediate | CsiIgnore, 0x40..=0x7e) => Ground,
        (DcsEntry | DcsParam, 0x20..=0x2f) => DcsIntermediate,
        (DcsEntry, 0x30..=0x3f) => DcsParam,
        (DcsParam, 0x3c..=0x3f) | (DcsIntermediate, 0x30..=0x3f) => DcsIgnore,
        (DcsEntry | DcsParam | DcsIntermediate, 0x40..=0x7e) => DcsPassthrough,
        (DcsPassthrough, 0x9c) | (OscString, 0x07) => Ground,
        (state, _) => state,
    }
}

fn runs_at_once(state: State, byte: u8) -> bool {
    use State::*;
    matches!(
        state,
        Escape | EscapeIntermediate | CsiEntry | CsiParam | CsiIntermediate | CsiIgnore
    ) && matches!(byte, 0x00..=0x17 | 0x19 | 0x1c..=0x1f)
}

// The length of a UTF-8 character that the end of `bytes` cuts.
fn open_utf8(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(3) {
        let width = match bytes[bytes.len() - back] {
            0x80..=0xbf => continue,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => 0,
        };
        return if width > back { back } else { 0 };
    }
    0
}
