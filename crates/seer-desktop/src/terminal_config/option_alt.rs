#[derive(Clone, Copy, Default)]
pub(crate) struct OptionAsAlt {
    left: bool,
    right: bool,
}

impl gpui::Global for OptionAsAlt {}

impl OptionAsAlt {
    pub(super) fn ghostty(value: &str) -> Self {
        match value {
            "true" => Self::sides(true, true),
            "left" => Self::sides(true, false),
            "right" => Self::sides(false, true),
            _ => Self::default(),
        }
    }

    pub(super) fn kitty(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "both" | "yes" | "true" | "y" => Self::sides(true, true),
            "left" => Self::sides(true, false),
            "right" => Self::sides(false, true),
            _ => Self::default(),
        }
    }

    pub(super) fn alacritty(value: &toml::Value) -> Self {
        let value = value
            .get("window")
            .and_then(|window| window.get("option_as_alt"))
            .and_then(toml::Value::as_str);
        match value {
            Some("Both") => Self::sides(true, true),
            Some("OnlyLeft") => Self::sides(true, false),
            Some("OnlyRight") => Self::sides(false, true),
            _ => Self::default(),
        }
    }

    pub(super) fn iterm(profile: &plist::Dictionary) -> Self {
        let left = profile
            .get("Option Key Sends")
            .and_then(plist::Value::as_signed_integer);
        let right = profile
            .get("Right Option Key Sends")
            .and_then(plist::Value::as_signed_integer)
            .or(left);
        Self::sides(left == Some(2), right == Some(2))
    }

    fn sides(left: bool, right: bool) -> Self {
        Self { left, right }
    }

    pub(crate) fn active(self, flags: usize) -> bool {
        // AppKit keeps the device-specific left and right Option bits in NSEvent flags.
        flags & (1 << 19) != 0
            && ((self.left && flags & 0x20 != 0) || (self.right && flags & 0x40 != 0))
    }
}
