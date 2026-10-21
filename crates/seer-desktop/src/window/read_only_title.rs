use gpui::{
    FontWeight, IntoElement, Pixels, ShapedLine, SharedString, Styled, TextStyle, Window, canvas,
    px, rgb,
};

use crate::{label, palette};

pub(super) fn title(person: SharedString, terminal: Option<SharedString>) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let style = window.text_style();
            let mut person_style = style.clone();
            person_style.font_weight = FontWeight::NORMAL;
            person_style.color = rgb(palette::MUTED).into();
            let mut parts = vec![(person, person_style.clone())];
            if let Some(terminal) = terminal {
                person_style.color = rgb(palette::FAINTER).into();
                parts.push(("/".into(), person_style));
                parts.push((terminal, style.clone()));
            }
            let lines = truncate(parts, bounds.size.width, window);
            let width = lines.iter().fold(px(0.), |width, line| width + line.width)
                + px(6.) * lines.len().saturating_sub(1) as f32;
            let (left, right) = ink_span(&lines, window).unwrap_or((px(0.), width));
            let scale = window.scale_factor();
            let mut x =
                ((bounds.left() + (bounds.size.width - left - right) / 2.) * scale).round() / scale;
            lines
                .into_iter()
                .map(|line| {
                    let font_size = style.font_size.to_pixels(window.rem_size());
                    let height = style
                        .line_height
                        .to_pixels(font_size.into(), window.rem_size());
                    let baseline =
                        label::baseline(bounds, label::cap_height(&line, window), px(0.), scale);
                    let mut origin = label::origin(&line, bounds, height, baseline, scale);
                    origin.x = x;
                    x += line.width + px(6.);
                    (line, origin, height)
                })
                .collect::<Vec<_>>()
        },
        |_, lines, window, cx| {
            for (line, origin, height) in lines {
                if let Err(error) = line.paint(origin, height, window, cx) {
                    eprintln!("seer: could not draw title: {error}");
                }
            }
        },
    )
    .w(px(504.))
    .min_w_0()
    .h_full()
    .overflow_hidden()
}

fn truncate(
    mut parts: Vec<(SharedString, TextStyle)>,
    width: Pixels,
    window: &Window,
) -> Vec<ShapedLine> {
    for (text, _) in &mut parts {
        if let Some((first, _)) = text.split_once('\n') {
            *text = first.to_owned().into();
        }
    }
    let size = window.text_style().font_size.to_pixels(window.rem_size());
    let mut lines: Vec<_> = parts
        .iter()
        .map(|(text, style)| {
            window
                .text_system()
                .shape_line(text.clone(), size, &[style.to_run(text.len())], None)
        })
        .collect();
    let separator = if parts.len() == 3 {
        lines[1].width
    } else {
        px(0.)
    };
    let fixed = px(6.) * parts.len().saturating_sub(1) as f32 + separator;
    let flexible = lines.iter().fold(px(0.), |width, line| width + line.width) - separator;
    let ratio = f32::from((width - fixed).max(px(0.))) / f32::from(flexible.max(px(1.)));
    for (index, ((text, style), line)) in parts.into_iter().zip(&mut lines).enumerate() {
        if ratio >= 1. || index == 1 {
            continue;
        }
        let mut runs = vec![style.to_run(text.len())];
        let text = window
            .text_system()
            .line_wrapper(style.font(), size)
            .truncate_line(text, line.width * ratio, "\u{2026}", &mut runs);
        *line = window.text_system().shape_line(text, size, &runs, None);
    }
    lines
}

fn ink_span(lines: &[ShapedLine], window: &Window) -> Option<(Pixels, Pixels)> {
    let mut offset = px(0.);
    let mut span: Option<(Pixels, Pixels)> = None;
    for line in lines {
        for (left, right) in line_ink(line, window) {
            let (left, right) = (offset + left, offset + right);
            span = Some(span.map_or((left, right), |(a, b)| (a.min(left), b.max(right))));
        }
        offset += line.width + px(6.);
    }
    span
}

fn line_ink<'a>(
    line: &'a ShapedLine,
    window: &'a Window,
) -> impl Iterator<Item = (Pixels, Pixels)> + 'a {
    line.runs.iter().flat_map(move |run| {
        run.glyphs.iter().filter_map(move |glyph| {
            let character = line.text[glyph.index..].chars().next()?;
            let ink = window
                .text_system()
                .typographic_bounds(run.font_id, line.font_size, character)
                .ok()?;
            (ink.size.width > px(0.)).then_some((
                glyph.position.x + ink.left(),
                glyph.position.x + ink.right(),
            ))
        })
    })
}
