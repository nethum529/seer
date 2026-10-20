use super::{
    SeerWindow,
    shortcuts_native_e2e::{baseline_probe, text_width},
};
use gpui::{FontWeight, Window};
use std::{fs, path::Path};

pub(super) fn record(view: &SeerWindow, window: &Window, evidence: &Path, capture: &str) {
    let path = evidence.join("process-samples.json");
    let mut samples: Vec<serde_json::Value> = if capture == "processes-rows" {
        Vec::new()
    } else {
        fs::read(&path)
            .map(|bytes| serde_json::from_slice(&bytes).unwrap())
            .unwrap_or_default()
    };
    let mut sample = |name: &str,
                      text: &str,
                      x: f32,
                      y: f32,
                      height: f32,
                      size: f32,
                      weight: FontWeight| {
        let ink = text
            .char_indices()
            .find(|(_, ch)| ch.is_uppercase() || ch.is_ascii_digit() || *ch == 'l')
            .unwrap_or_else(|| text.char_indices().next().unwrap());
        let offset = text_width(window, &text[..ink.0], size, weight);
        let width = text_width(window, &ink.1.to_string(), size, weight)
            .floor()
            .max(2.);
        samples.push(serde_json::json!({"capture":capture,"name":format!("{capture} {name}"),"box":[x+offset,y,width,height],"probe":baseline_probe(window,text,x,size,weight),"dark_ink":name == "Confirm"}));
    };
    if capture == "processes-stop" {
        let right = 659.;
        let confirm = text_width(window, "Stop runtime", 13., FontWeight::MEDIUM) + 28.;
        let cancel = text_width(window, "Cancel", 13., FontWeight::MEDIUM) + 28.;
        sample(
            "Sheet title",
            "Stop this runtime?",
            301.,
            271.,
            20.,
            15.,
            FontWeight::SEMIBOLD,
        );
        sample(
            "Sheet body",
            "Its shells will end. Its windows lose their terminals.",
            301.,
            299.,
            18.,
            13.,
            FontWeight::NORMAL,
        );
        sample(
            "Cancel",
            "Cancel",
            right - confirm - 8. - cancel + 14.,
            337.,
            32.,
            13.,
            FontWeight::MEDIUM,
        );
        sample(
            "Confirm",
            "Stop runtime",
            right - confirm + 14.,
            337.,
            32.,
            13.,
            FontWeight::MEDIUM,
        );
    } else {
        for (name, text, x, y, size, weight) in [
            (
                "Settings heading",
                "Settings",
                145.,
                65.,
                15.,
                FontWeight::SEMIBOLD,
            ),
            (
                "Appearance section",
                "Appearance",
                145.,
                113.,
                13.,
                FontWeight::MEDIUM,
            ),
            (
                "Shortcuts section",
                "Shortcuts",
                145.,
                145.,
                13.,
                FontWeight::MEDIUM,
            ),
            (
                "Processes section",
                "Processes",
                145.,
                177.,
                13.,
                FontWeight::MEDIUM,
            ),
            (
                "Processes title",
                "Processes",
                321.,
                65.,
                15.,
                FontWeight::SEMIBOLD,
            ),
        ] {
            sample(name, text, x, y, 32., size, weight);
        }
        let scroll = &view.appearance.scroll;
        let bounds = scroll.bounds_for_item(0).unwrap() + scroll.offset();
        if bounds.top() >= scroll.bounds().top() {
            sample(
                "Refresh",
                "Refresh",
                f32::from(bounds.left()) + 14.,
                f32::from(bounds.top()),
                32.,
                13.,
                FontWeight::MEDIUM,
            );
            if view.appearance.processes.can_clean() {
                let x = f32::from(bounds.left())
                    + text_width(window, "Refresh", 13., FontWeight::MEDIUM)
                    + 28.
                    + 8.
                    + 14.;
                sample(
                    "Clean up",
                    "Clean up",
                    x,
                    f32::from(bounds.top()),
                    32.,
                    13.,
                    FontWeight::MEDIUM,
                );
            }
        }
        for (index, row) in view.appearance.processes.rows.iter().enumerate() {
            let bounds = scroll.bounds_for_item(index + 1).unwrap() + scroll.offset();
            if bounds.top() < scroll.bounds().top() || bounds.bottom() > scroll.bounds().bottom() {
                continue;
            }
            let kind = match row.process.kind() {
                seer::processes::Kind::Broker => "Room server",
                seer::processes::Kind::Runtime => "Runtime",
                seer::processes::Kind::Window => "Window",
            };
            let x = f32::from(bounds.left());
            let y = f32::from(bounds.top());
            sample(
                &format!("Row {index} kind"),
                kind,
                x,
                y,
                32.,
                13.,
                FontWeight::MEDIUM,
            );
            sample(
                &format!("Row {index} PID"),
                &row.process.pid().to_string(),
                x + text_width(window, kind, 13., FontWeight::MEDIUM) + 8.,
                y,
                32.,
                12.,
                FontWeight::NORMAL,
            );
            if row.process.description() != "-" {
                sample(
                    &format!("Row {index} detail"),
                    &row.process.description(),
                    x,
                    y + 36.,
                    18.,
                    13.,
                    FontWeight::NORMAL,
                );
            }
            sample(
                &format!("Row {index} uptime CPU"),
                "Up",
                x,
                y + 58.,
                16.,
                12.,
                FontWeight::NORMAL,
            );
            if row.can_stop() {
                sample(
                    &format!("Row {index} Stop"),
                    "Stop",
                    f32::from(bounds.right())
                        - text_width(window, "Stop", 13., FontWeight::MEDIUM)
                        - 14.,
                    y,
                    32.,
                    13.,
                    FontWeight::MEDIUM,
                );
            } else if row.process.used_by(std::process::id()) {
                sample(
                    "Used by this window",
                    "Used by this window",
                    f32::from(bounds.right())
                        - text_width(window, "Used by this window", 12., FontWeight::NORMAL),
                    y,
                    32.,
                    12.,
                    FontWeight::NORMAL,
                );
            }
        }
    }
    fs::write(path, serde_json::to_vec_pretty(&samples).unwrap()).unwrap();
}
