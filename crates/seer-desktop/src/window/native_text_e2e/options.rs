use gpui::px;

pub(crate) fn window_options(
    mut options: gpui::WindowOptions,
    cx: &gpui::App,
) -> gpui::WindowOptions {
    if std::env::var_os("TEXT_PIXEL_START_REVIEW_ONLY").is_some()
        || std::env::var_os("PROCESSES_QA").is_some()
        || std::env::var_os("SHORTCUT_KEYS_QA").is_some()
    {
        // Nonactivating panels can receive native keys while the test app is in the background.
        options.kind = gpui::WindowKind::PopUp;
    }
    if let Ok(secondary) = std::env::var("TEXT_PIXEL_SECONDARY") {
        let display = cx
            .displays()
            .into_iter()
            .find(|display| {
                display.id() != cx.primary_display().unwrap().id()
                    && (secondary == "1" || u32::from(display.id()).to_string() == secondary)
            })
            .expect("a secondary native display is required");
        options.display_id = Some(display.id());
        let mut bounds =
            gpui::Bounds::centered(Some(display.id()), gpui::size(px(960.), px(640.)), cx);
        if ["SHORTCUT_KEYS_QA", "FONT_QA", "COLOR_QA"]
            .iter()
            .any(|key| std::env::var_os(key).is_some())
        {
            // GPUI 0.2.2 adds the macOS display origin when it creates a window.
            bounds.origin -= display.bounds().origin;
        }
        options.window_bounds = Some(gpui::WindowBounds::Windowed(bounds));
    }
    options
}
