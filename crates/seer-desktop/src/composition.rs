use std::ops::Range;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use crate::link::lock;
use crate::window::{Content, SeerWindow};

#[derive(Default)]
pub(crate) struct Composition {
    pub(crate) text: String,
    selection: Range<usize>,
}

impl Composition {
    pub(crate) fn marked(&self) -> bool {
        !self.text.is_empty()
    }

    fn len(&self) -> usize {
        self.text.encode_utf16().count()
    }

    fn replace(&mut self, range: Option<Range<usize>>, text: &str) -> usize {
        let range = range.unwrap_or(0..self.len());
        let start = byte_offset(&self.text, range.start);
        let end = byte_offset(&self.text, range.end).max(start);
        self.text.replace_range(start..end, text);
        range.start.min(self.len())
    }
}

impl EntityInputHandler for SeerWindow {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let mark = &self.input.composition;
        let start = byte_offset(&mark.text, range.start);
        let end = byte_offset(&mark.text, range.end).max(start);
        *adjusted = Some(
            mark.text[..start].encode_utf16().count()..mark.text[..end].encode_utf16().count(),
        );
        Some(mark.text[start..end].into())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.input.composition.selection.clone(),
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let mark = &self.input.composition;
        mark.marked().then(|| 0..mark.len())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.input.composition = Composition::default();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let marked = self.input.composition.marked();
        self.input.composition.replace(range, text);
        let committed = std::mem::take(&mut self.input.composition).text;
        if !committed.is_empty() {
            self.send_input(seer_core::InputEvent::Text(committed), cx);
        }
        if marked {
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input.composition.marked() && !text.is_empty() {
            self.with_raw(|grid| grid.input_started());
        }
        let mark = &mut self.input.composition;
        let start = mark.replace(range, text);
        mark.selection = selected.map_or_else(
            || mark.len()..mark.len(),
            |selected| {
                (start + selected.start).min(mark.len())..(start + selected.end).min(mark.len())
            },
        );
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.cursor_bounds()
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

impl SeerWindow {
    pub(crate) fn cursor_bounds(&self) -> Option<Bounds<Pixels>> {
        let grid = self.input.grid?;
        let Content::Terminal(link, _) = &self.content else {
            return None;
        };
        let held = lock(link.terminal());
        let cursor = held.as_ref()?.cursor();
        Some(grid.cell(usize::from(cursor.column), usize::from(cursor.row), 1))
    }
}

fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut count = 0;
    for (index, character) in text.char_indices() {
        if count >= utf16 {
            return index;
        }
        count += character.len_utf16();
    }
    text.len()
}
