use std::borrow::Cow;

use alacritty_terminal::vte::{Params, Parser, Perform};

#[derive(Default)]
pub(super) struct KeyboardGuard {
    parser: Parser,
    depth: Depth,
}

impl KeyboardGuard {
    pub(super) fn filter<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        let mut filtered = Cow::Borrowed(bytes);
        let mut offset = 0;
        while offset < bytes.len() {
            self.depth.cancel = false;
            offset += self
                .parser
                .advance_until_terminated(&mut self.depth, &bytes[offset..]);
            if self.depth.cancel {
                filtered.to_mut()[offset - 1] = 0x18;
            }
        }
        filtered
    }
}

#[derive(Default)]
struct Depth {
    screens: [usize; 2],
    alternate: bool,
    cancel: bool,
}

impl Perform for Depth {
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }
        let depth = &mut self.screens[usize::from(self.alternate)];
        match (action, intermediates) {
            ('u', [b'>']) => {
                // alacritty_terminal 0.26 removes from title_stack at depth 4096.
                if *depth == 4096 {
                    self.cancel = true;
                } else {
                    *depth += 1;
                }
            }
            ('u', [b'<']) => {
                let count = params.iter().next().map_or(1, |param| param[0].max(1));
                *depth = depth.saturating_sub(usize::from(count));
            }
            ('h' | 'l', [b'?']) if params.iter().any(|param| param[0] == 1049) => {
                self.alternate = action == 'h';
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _: bool, byte: u8) {
        if byte == b'c' && intermediates.is_empty() {
            *self = Self::default();
        }
    }

    fn terminated(&self) -> bool {
        self.cancel
    }
}
