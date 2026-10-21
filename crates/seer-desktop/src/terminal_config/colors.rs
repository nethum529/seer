use crate::palette::TerminalColors;

fn hex(value: &str) -> Option<u32> {
    let value = value
        .strip_prefix('#')
        .or_else(|| value.strip_prefix("0x"))
        .unwrap_or(value);
    (value.len() == 6 && value.bytes().all(|ch| ch.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(value, 16).ok())
        .flatten()
}

fn rgb(value: &str) -> Option<u32> {
    let value = value.trim();
    if let Some(value) = value.strip_prefix("rgb:") {
        let channels: Vec<_> = value.split('/').collect();
        if channels.len() != 3 {
            return None;
        }
        return channels
            .into_iter()
            .try_fold(0, |color, part| Some(color << 8 | component(part)?));
    }
    if let Some(value) = value.strip_prefix('#').or_else(|| {
        value
            .bytes()
            .all(|ch| ch.is_ascii_hexdigit())
            .then_some(value)
    }) {
        if !matches!(value.len(), 3 | 6 | 9 | 12) {
            return None;
        }
        let width = value.len() / 3;
        return value.as_bytes().chunks(width).try_fold(0, |color, part| {
            Some(color << 8 | component(std::str::from_utf8(part).ok()?)?)
        });
    }
    let name: String = value
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    include_str!("x11-colors.txt")
        .split_ascii_whitespace()
        .find_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            (key == name).then(|| hex(value)).flatten()
        })
}

fn component(value: &str) -> Option<u32> {
    if value.is_empty() || value.len() > 4 || !value.bytes().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let number = u32::from_str_radix(value, 16).ok()?;
    let maximum = (1 << (value.len() * 4)) - 1;
    Some((number * 255 + maximum / 2) / maximum)
}

fn palette_index(value: &str) -> Option<usize> {
    let value = value.trim();
    let (digits, radix) = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(prefix, radix)| value.strip_prefix(prefix).map(|digits| (digits, radix)))
        .unwrap_or((value, 10));
    usize::from_str_radix(digits, radix)
        .ok()
        .filter(|index| *index < 16)
}

pub(super) fn line(colors: &mut TerminalColors, key: &str, value: &str, ghostty: bool) {
    if let Some((index, value)) = key
        .strip_prefix("color")
        .filter(|_| !ghostty)
        .map(|index| (index, value))
        .or_else(|| {
            (ghostty && key == "palette")
                .then(|| value.split_once('='))
                .flatten()
        })
    {
        if let Some(index) = palette_index(index)
            && let Some(color) = rgb(value.trim())
        {
            colors.ansi[index] = color;
        }
        return;
    }
    let field = match key {
        "foreground" => &mut colors.foreground,
        "background" => &mut colors.background,
        "cursor-color" if ghostty => &mut colors.cursor,
        "cursor" if !ghostty => &mut colors.cursor,
        _ => return,
    };
    if let Some(color) = rgb(value) {
        *field = Some(color);
    }
}

pub(super) fn alacritty(values: &toml::Value) -> TerminalColors {
    let mut colors = TerminalColors::default();
    let Some(table) = values.get("colors") else {
        return colors;
    };
    let read = |group: &str, key: &str| table.get(group)?.get(key)?.as_str().and_then(hex);
    colors.foreground = read("primary", "foreground");
    colors.background = read("primary", "background");
    colors.cursor = read("cursor", "cursor");
    for (offset, group) in [(0, "normal"), (8, "bright")] {
        for (index, key) in [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ]
        .iter()
        .enumerate()
        {
            if let Some(color) = read(group, key) {
                colors.ansi[offset + index] = color;
            }
        }
    }
    colors
}

#[cfg(target_os = "macos")]
pub(super) fn iterm(profile: &plist::Dictionary) -> TerminalColors {
    let mut colors = TerminalColors::default();
    for (index, color) in colors.ansi.iter_mut().enumerate() {
        if let Some(value) = iterm_color(profile, &format!("Ansi {index} Color")) {
            *color = value;
        }
    }
    colors.foreground = iterm_color(profile, "Foreground Color");
    colors.background = iterm_color(profile, "Background Color");
    colors.cursor = iterm_color(profile, "Cursor Color");
    colors
}

#[cfg(target_os = "macos")]
fn iterm_color(profile: &plist::Dictionary, key: &str) -> Option<u32> {
    let color = profile.get(key)?.as_dictionary()?;
    let mut channels = [0.; 3];
    for (channel, key) in
        channels
            .iter_mut()
            .zip(["Red Component", "Green Component", "Blue Component"])
    {
        let value = color.get(key)?;
        *channel = value
            .as_real()
            .or_else(|| value.as_signed_integer().map(|value| value as f64))?;
        if !channel.is_finite() || !(0. ..=1.).contains(channel) {
            return None;
        }
    }
    let space = color.get("Color Space").and_then(plist::Value::as_string);
    if space != Some("sRGB") {
        use objc2_app_kit::{NSColor, NSColorSpace};
        let [red, green, blue] = channels;
        let native = match space {
            Some("P3") => NSColor::colorWithDisplayP3Red_green_blue_alpha(red, green, blue, 1.),
            None | Some("Calibrated") => {
                NSColor::colorWithCalibratedRed_green_blue_alpha(red, green, blue, 1.)
            }
            _ => return None,
        };
        let converted = native.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
        channels = [
            converted.redComponent(),
            converted.greenComponent(),
            converted.blueComponent(),
        ];
    }
    Some(channels.into_iter().fold(0, |color, channel| {
        color << 8 | (channel.clamp(0., 1.) * 255.).round() as u32
    }))
}
