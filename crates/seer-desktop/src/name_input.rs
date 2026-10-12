use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, Pixels, Point, Render,
    SharedString, Styled, UTF16Selection, Window, div, px, rgb,
};

use crate::palette;

mod paint;

pub(crate) enum Finished {
    Save,
    Cancel,
    SaveOutside,
    Changed,
}

pub(crate) struct NameInput {
    pub(crate) focus: FocusHandle,
    pub(crate) text: String,
    placeholder: SharedString,
    selection: Range<usize>,
    anchor: usize,
    marked: Option<Range<usize>>,
    line: Option<gpui::ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    offset: Pixels,
    dragging: bool,
    pub(crate) centered: bool,
    form: bool,
    code: bool,
    limit: usize,
    pub(crate) invalid: bool,
    pub(crate) disabled: bool,
}

impl EventEmitter<Finished> for NameInput {}

impl NameInput {
    pub(crate) fn new(text: String, placeholder: String, cx: &mut Context<Self>) -> Self {
        let end = text.len();
        Self {
            focus: cx.focus_handle(),
            text,
            placeholder: placeholder.into(),
            selection: 0..end,
            anchor: 0,
            marked: None,
            line: None,
            bounds: None,
            offset: px(0.),
            dragging: false,
            centered: false,
            form: false,
            code: false,
            limit: 40,
            invalid: false,
            disabled: false,
        }
    }

    pub(crate) fn form(text: String, code: bool, cx: &mut Context<Self>) -> Self {
        let mut input = Self::new(
            text,
            if code {
                "seer join SEER2-..."
            } else {
                "Your name"
            }
            .into(),
            cx,
        );
        input.form = true;
        input.code = code;
        input.limit = if code { 4096 } else { 32 };
        input.focus = cx.focus_handle().tab_stop(true);
        input
    }

    fn select(&mut self, at: usize, extend: bool) {
        if !extend {
            self.anchor = at;
        }
        self.selection = self.anchor.min(at)..self.anchor.max(at);
    }

    fn cursor(&self) -> usize {
        if self.anchor == self.selection.end {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn adjacent(&self, at: usize, left: bool) -> usize {
        if left {
            self.text[..at]
                .char_indices()
                .next_back()
                .map_or(0, |(at, _)| at)
        } else {
            self.text[at..]
                .chars()
                .next()
                .map_or(at, |ch| at + ch.len_utf8())
        }
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            cx.stop_propagation();
            return;
        }
        let key = &event.keystroke;
        let command = key.modifiers.platform || key.modifiers.control;
        match key.key.as_str() {
            "enter" => cx.emit(Finished::Save),
            "escape" => cx.emit(Finished::Cancel),
            "a" if command => {
                self.anchor = 0;
                self.selection = 0..self.text.len();
            }
            "c" | "x" if command => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.text[self.selection.clone()].to_owned(),
                ));
                if key.key == "x" {
                    self.replace_text_in_range(None, "", window, cx);
                }
            }
            "v" if command => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace_text_in_range(None, &text, window, cx);
                }
            }
            "backspace" | "delete" => {
                if self.selection.is_empty() {
                    let at = self.adjacent(self.cursor(), key.key == "backspace");
                    self.select(at, true);
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "left" | "right" | "home" | "end" => self.navigate(key),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn navigate(&mut self, key: &gpui::Keystroke) {
        let left = key.key == "left";
        let at = match key.key.as_str() {
            "home" => 0,
            "end" => self.text.len(),
            _ if key.modifiers.platform => {
                if left {
                    0
                } else {
                    self.text.len()
                }
            }
            _ if !key.modifiers.shift && !self.selection.is_empty() => {
                if left {
                    self.selection.start
                } else {
                    self.selection.end
                }
            }
            _ => self.adjacent(self.cursor(), left),
        };
        self.select(at, key.modifiers.shift);
    }

    fn mouse_index(&self, position: Point<Pixels>) -> usize {
        self.bounds
            .zip(self.line.as_ref())
            .map_or(0, |(bounds, line)| {
                line.closest_index_for_x(position.x - bounds.left() + self.offset)
            })
            .min(self.text.len())
    }

    fn utf8(&self, offset: usize) -> usize {
        let mut units = 0;
        self.text
            .char_indices()
            .find_map(|(at, ch)| {
                if units >= offset {
                    Some(at)
                } else {
                    units += ch.len_utf16();
                    None
                }
            })
            .unwrap_or(self.text.len())
    }

    fn utf16(&self, offset: usize) -> usize {
        self.text[..offset].encode_utf16().count()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.utf8(range.start)..self.utf8(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.utf16(range.start)..self.utf16(range.end)
    }

    fn replace(&mut self, range: Range<usize>, text: &str) -> usize {
        let remaining = self
            .limit
            .saturating_sub(self.text.chars().count() - self.text[range.clone()].chars().count());
        let text: String = text
            .chars()
            .filter(|ch| !ch.is_control())
            .take(remaining)
            .collect();
        let end = range.start + text.len();
        self.text.replace_range(range, &text);
        self.select(end, false);
        end
    }
}

impl EntityInputHandler for NameInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        *actual = Some(self.range_to_utf16(&range));
        Some(self.text[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selection),
            reversed: self.anchor == self.selection.end,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let range = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked.take())
            .unwrap_or(self.selection.clone());
        self.replace(range, text);
        self.marked = None;
        if self.form {
            cx.emit(Finished::Changed);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let range = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked.take())
            .unwrap_or(self.selection.clone());
        let start = range.start;
        let end = self.replace(range, text);
        if self.form {
            cx.emit(Finished::Changed);
        }
        self.marked = Some(start..end);
        if let Some(selected) = selected {
            let base = self.utf16(start);
            let selected = self.range_from_utf16(&(base + selected.start..base + selected.end));
            self.anchor = selected.start;
            self.selection = selected;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range);
        let line = self.line.as_ref()?;
        Some(Bounds::from_corners(
            gpui::point(
                bounds.left() + line.x_for_index(range.start) - self.offset,
                bounds.top(),
            ),
            gpui::point(
                bounds.left() + line.x_for_index(range.end) - self.offset,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.utf16(self.mouse_index(point)))
    }
}

impl Render for NameInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(if self.form {
                "join-field"
            } else {
                "name-input"
            })
            .debug_selector(|| {
                if self.form {
                    "join-field"
                } else {
                    "name-input"
                }
                .into()
            })
            .track_focus(&self.focus)
            .cursor_text()
            .w_full()
            .min_w_0()
            .h(px(22.))
            .px(px(5.))
            .my(px(-2.))
            .ml(px(if self.centered { 0. } else { -6. }))
            .rounded(px(6.))
            .border_1()
            .border_color(gpui::rgba((palette::ACCENT << 8) | 0x73))
            .bg(rgb(palette::WINDOW))
            .overflow_hidden()
            .line_height(px(20.))
            .when(self.form, |field| {
                field
                    .h(px(36.))
                    .px(px(11.))
                    .my(px(0.))
                    .ml(px(0.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .border_color(if self.invalid {
                        gpui::rgba(0xf871718c)
                    } else if self.focus.is_focused(window) {
                        gpui::rgba(0xfafafa73)
                    } else {
                        rgb(palette::LINE_2)
                    })
            })
            .when(self.code, |field| {
                field.font_family("JetBrains Mono").text_size(px(12.))
            })
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|input, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&input.focus);
                    input.dragging = true;
                    if event.click_count >= 2 {
                        input.anchor = 0;
                        input.selection = 0..input.text.len();
                    } else {
                        input.select(input.mouse_index(event.position), event.modifiers.shift);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|input, _, _, _| input.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|input, _, _, _| input.dragging = false),
            )
            .on_mouse_move(cx.listener(|input, event: &gpui::MouseMoveEvent, _, cx| {
                if input.dragging {
                    input.select(input.mouse_index(event.position), true);
                    cx.notify();
                }
            }))
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(Finished::SaveOutside)))
            .child(paint::text(cx.entity()))
    }
}
