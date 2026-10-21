use gpui::{App, Bounds, Font, Pixels, Point, ShapedLine, Window, font, px, size};

pub(crate) const DEFAULT_SIZE: f32 = 12.5;
pub(crate) const LINE_RATIO: f32 = 19. / DEFAULT_SIZE;

pub(crate) fn default_font() -> Font {
    let mut font = font("JetBrains Mono");
    font.features = gpui::FontFeatures::disable_ligatures();
    font
}

pub(crate) fn load(cx: &App) -> (Font, f32) {
    let settings = crate::terminal_config::load();
    let mut font = default_font();
    if !settings.families.is_empty() {
        let names = cx.text_system().all_font_names();
        let family = settings
            .families
            .iter()
            .find_map(|family| installed_family(family, &names));
        let Some(family) = family else {
            return (font, DEFAULT_SIZE);
        };
        font.family = family.into();
    }
    let size = settings
        .size
        .filter(|size| size.is_finite() && *size > 0. && *size <= 256.)
        .map(|size| {
            if cfg!(target_os = "linux") {
                size * 96. / 72.
            } else {
                size
            }
        })
        .unwrap_or(DEFAULT_SIZE);
    (font, size)
}

fn installed_family(name: &str, families: &[String]) -> Option<String> {
    if let Some(family) = families
        .iter()
        .find(|family| family.eq_ignore_ascii_case(name))
    {
        return Some(family.clone());
    }
    let source = font_kit::source::SystemSource::new();
    source.all_fonts().ok()?.into_iter().find_map(|handle| {
        let font = handle.load().ok()?;
        let matches = font
            .postscript_name()
            .is_some_and(|face| face.eq_ignore_ascii_case(name))
            || font.full_name().eq_ignore_ascii_case(name);
        matches
            .then(|| font.family_name())
            .filter(|family| families.contains(family))
    })
}

pub(crate) fn paint(
    line: &ShapedLine,
    origin: Point<Pixels>,
    height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let bounds = Bounds::new(origin, size(line.width, height));
    let scale = window.scale_factor();
    let capital = crate::label::capital_height(line, 'H', window);
    let baseline = crate::label::baseline(bounds, capital, px(0.), scale);
    let origin = crate::label::origin(line, bounds, height, baseline, scale);
    line.paint(origin, height, window, cx).ok();
}
