use super::{
    SeerWindow,
    settings::sliders,
    shortcuts_native_e2e::{baseline_probe, text_width},
};
use gpui::{FontWeight, Window, px};

pub(super) fn sliders(
    view: &SeerWindow,
    window: &Window,
    capture: &str,
    samples: &mut Vec<serde_json::Value>,
) {
    for (index, label) in ["Picture brightness", "Glass darkness", "Glass blur"]
        .into_iter()
        .enumerate()
    {
        let bounds = view.appearance.slider_bounds[index].get();
        let x = f32::from(bounds.left());
        let y = f32::from(bounds.top()) - 32.;
        let narrow = window.viewport_size().width < px(600.);
        let label_y = if narrow { y - 24. } else { y };
        let label_height = if narrow { 24. } else { 32. };
        let right = f32::from(bounds.right());
        let reset_width = text_width(window, "Reset", 13., FontWeight::MEDIUM);
        sample(
            window,
            capture,
            label,
            [x, label_y, label_height],
            FontWeight::MEDIUM,
            samples,
        );
        sample(
            window,
            capture,
            "Reset",
            [right - reset_width - 8., y, 32.],
            FontWeight::MEDIUM,
            samples,
        );
        let value = format!(
            "{}{}",
            sliders::value(&view.appearance.settings, index),
            if index == 2 { " px" } else { "%" }
        );
        let mut face = gpui::font("Inter");
        face.features = gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        let width = window
            .text_system()
            .shape_line(
                value.clone().into(),
                px(13.),
                &[gpui::TextRun {
                    len: value.len(),
                    font: face,
                    color: gpui::rgb(0xffffff).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width;
        sample(
            window,
            capture,
            &value,
            [right - reset_width - 16. - 8. - f32::from(width), y, 32.],
            FontWeight::NORMAL,
            samples,
        );
    }
}

pub(super) fn neighbors(
    view: &SeerWindow,
    window: &Window,
    capture: &str,
    samples: &mut Vec<serde_json::Value>,
) {
    for (label, x, y, size, weight) in [
        ("Settings", 145., 65., 15., FontWeight::SEMIBOLD),
        ("Appearance", 321., 65., 15., FontWeight::SEMIBOLD),
        ("Appearance", 145., 113., 13., FontWeight::MEDIUM),
        ("Shortcuts", 145., 145., 13., FontWeight::MEDIUM),
    ] {
        measure(window, capture, label, [x, y, 32.], (size, weight), samples);
    }
    let glass = view.appearance.settings.glass;
    let start = if glass { 4 } else { 1 };
    let mut rows = vec![(0, "Solid background", Some(!glass))];
    rows.extend([
        (
            start,
            "Uniform glass",
            Some(view.appearance.settings.uniform_glass),
        ),
        (
            start + 1,
            "Hide top bar",
            Some(view.appearance.settings.hide_top_bar),
        ),
        (
            start + 2,
            "Hide lines",
            Some(view.appearance.settings.hide_lines),
        ),
    ]);
    if glass {
        rows.extend([(7, "Background picture", None), (8, "Choose picture", None)]);
    }
    for (index, label, on) in rows {
        let bounds = view.appearance.scroll.bounds_for_item(index).unwrap()
            + view.appearance.scroll.offset();
        let viewport = view.appearance.scroll.bounds();
        if bounds.top() < viewport.top() || bounds.bottom() > viewport.bottom() {
            continue;
        }
        let x = f32::from(bounds.left());
        let y = f32::from(bounds.top());
        let height = if index == 7 { 18. } else { 32. };
        sample(
            window,
            capture,
            label,
            [x + if index == 8 { 14. } else { 0. }, y, height],
            FontWeight::MEDIUM,
            samples,
        );
        if let Some(on) = on {
            sample(
                window,
                capture,
                if on { "On" } else { "Off" },
                [f32::from(bounds.right()) - 70., y, 32.],
                FontWeight::NORMAL,
                samples,
            );
        }
    }
}

fn sample(
    window: &Window,
    capture: &str,
    text: &str,
    bounds: [f32; 3],
    weight: FontWeight,
    samples: &mut Vec<serde_json::Value>,
) {
    measure(window, capture, text, bounds, (13., weight), samples);
}

fn measure(
    window: &Window,
    capture: &str,
    text: &str,
    bounds: [f32; 3],
    style: (f32, FontWeight),
    samples: &mut Vec<serde_json::Value>,
) {
    let (size, weight) = style;
    let [x, y, height] = bounds;
    let width = text_width(window, &text[..1], size, weight).floor().max(2.);
    let probe = baseline_probe(window, text, x, size, weight);
    let label = format!("{capture} - {text} - {}", samples.len());
    samples.push(
        serde_json::json!({"capture":capture,"name":label,"box":[x,y,width,height],"probe":probe,"logical_width":f32::from(window.viewport_size().width)}),
    );
}
