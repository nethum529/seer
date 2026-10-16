use gpui::prelude::*;
use gpui::{
    ElementInputHandler, Entity, IntoElement, TextRun, UnderlineStyle, canvas, fill, point, px, rgb,
};

use super::NameInput;
use crate::palette;

pub(super) fn text(input: Entity<NameInput>) -> impl IntoElement {
    let preparing = input.clone();
    canvas(
        move |bounds, window, cx| {
            let input = preparing.read(cx);
            let style = window.text_style();
            let empty = input.text.is_empty();
            let text = if empty {
                input.placeholder.clone()
            } else {
                input.text.clone().into()
            };
            let run = TextRun {
                len: text.len(),
                font: style.font(),
                color: rgb(if empty { palette::FAINT } else { palette::TEXT }).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = if let Some(marked) = &input.marked {
                vec![
                    TextRun {
                        len: marked.start,
                        ..run.clone()
                    },
                    TextRun {
                        len: marked.len(),
                        underline: Some(UnderlineStyle {
                            color: None,
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..run.clone()
                    },
                    TextRun {
                        len: text.len() - marked.end,
                        ..run
                    },
                ]
                .into_iter()
                .filter(|run| run.len > 0)
                .collect()
            } else {
                vec![run]
            };
            let line = window.text_system().shape_line(
                text,
                px(if input.code { 12. } else { 13. }),
                &runs,
                None,
            );
            let cursor = line.x_for_index(input.cursor());
            let offset = input
                .offset
                .min(cursor)
                .max(cursor - bounds.size.width + px(1.))
                .max(px(0.));
            let offset = if input.centered && line.width < bounds.size.width {
                -(bounds.size.width - line.width) / 2.
            } else {
                offset
            };
            let scale = window.scale_factor();
            let baseline = crate::label::baseline(
                crate::label::bounds_for(bounds, cx),
                crate::label::cap_height(&line, window),
                px(0.),
                scale,
            );
            let origin = crate::label::origin(&line, bounds, px(20.), baseline, scale);
            (line, offset, origin)
        },
        move |bounds, (line, offset, mut origin), window, cx| {
            let held = input.read(cx);
            window.handle_input(
                &held.focus,
                ElementInputHandler::new(bounds, input.clone()),
                cx,
            );
            origin.x -= offset;
            if held.focus.is_focused(window) {
                let start = line.x_for_index(held.selection.start);
                let end = line.x_for_index(held.selection.end);
                let (from, to, color) = if held.selection.is_empty() {
                    (start, start + px(1.), rgb(palette::TEXT))
                } else {
                    (start, end, gpui::rgba(0x60a5fa73))
                };
                window.paint_quad(fill(
                    gpui::Bounds::from_corners(
                        point(origin.x + from, bounds.top()),
                        point(origin.x + to, bounds.bottom()),
                    ),
                    color,
                ));
            }
            if let Err(error) = line.paint(origin, px(20.), window, cx) {
                eprintln!("seer: could not draw terminal name: {error}");
            }
            input.update(cx, |input, _| {
                input.line = Some(line);
                input.bounds = Some(bounds);
                input.offset = offset;
            });
        },
    )
    .w_full()
    .h(px(20.))
}
