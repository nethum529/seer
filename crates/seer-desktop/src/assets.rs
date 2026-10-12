use std::borrow::Cow;

use gpui::{App, AssetSource, Result, SharedString};

pub(crate) struct Assets;

const ICONS: [(&str, &[u8]); 7] = [
    ("pencil", include_bytes!("../assets/icons/pencil.svg")),
    ("idle", include_bytes!("../assets/icons/idle.svg")),
    ("working", include_bytes!("../assets/icons/working.svg")),
    ("waiting", include_bytes!("../assets/icons/waiting.svg")),
    ("plus", include_bytes!("../assets/icons/plus.svg")),
    ("selector", include_bytes!("../assets/icons/selector.svg")),
    ("gear", include_bytes!("../assets/icons/gear.svg")),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS.iter().map(|(name, _)| (*name).into()).collect())
    }
}

pub(crate) fn load_fonts(cx: &App) -> Result<()> {
    let fonts: [&[u8]; 7] = [
        include_bytes!("../assets/fonts/inter/Inter-Regular.ttf"),
        include_bytes!("../assets/fonts/inter/Inter-Medium.ttf"),
        include_bytes!("../assets/fonts/inter/Inter-SemiBold.ttf"),
        include_bytes!("../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"),
        include_bytes!("../assets/fonts/jetbrains-mono/JetBrainsMono-Italic.ttf"),
        include_bytes!("../assets/fonts/jetbrains-mono/JetBrainsMono-SemiBold.ttf"),
        include_bytes!("../assets/fonts/jetbrains-mono/JetBrainsMono-SemiBoldItalic.ttf"),
    ];
    cx.text_system()
        .add_fonts(fonts.into_iter().map(Cow::Borrowed).collect())
}
