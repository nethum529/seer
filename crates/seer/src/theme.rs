use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders};

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) accent: Color,
    pub(crate) blue: Color,
    pub(crate) surface0: Color,
    pub(crate) surface1: Color,
    pub(crate) overlay0: Color,
    pub(crate) text: Color,
    pub(crate) subtext0: Color,
    pub(crate) green: Color,
    pub(crate) yellow: Color,
    pub(crate) red: Color,
}

impl Default for Palette {
    fn default() -> Self {
        if std::env::var("COLORTERM")
            .is_ok_and(|value| matches!(value.as_str(), "truecolor" | "24bit"))
        {
            Self::mocha()
        } else {
            Self::terminal()
        }
    }
}

impl Palette {
    pub(crate) fn mocha() -> Self {
        Self {
            accent: Color::Rgb(137, 180, 250),
            blue: Color::Rgb(137, 180, 250),
            surface0: Color::Rgb(49, 50, 68),
            surface1: Color::Rgb(69, 71, 90),
            overlay0: Color::Rgb(108, 112, 134),
            text: Color::Rgb(205, 214, 244),
            subtext0: Color::Rgb(166, 173, 200),
            green: Color::Rgb(166, 227, 161),
            yellow: Color::Rgb(249, 226, 175),
            red: Color::Rgb(243, 139, 168),
        }
    }

    pub(crate) fn terminal() -> Self {
        Self {
            accent: Color::LightBlue,
            blue: Color::LightBlue,
            surface0: Color::DarkGray,
            surface1: Color::DarkGray,
            overlay0: Color::Gray,
            text: Color::White,
            subtext0: Color::Gray,
            green: Color::LightGreen,
            yellow: Color::LightYellow,
            red: Color::LightRed,
        }
    }

    pub(crate) fn style(self) -> Style {
        Style::default().fg(self.text).bg(Color::Reset)
    }

    pub(crate) fn block(self, focused: bool) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .style(self.style())
            .border_style(
                self.style()
                    .fg(if focused { self.accent } else { self.overlay0 }),
            )
    }

    pub(crate) fn clear(self, buffer: &mut ratatui::buffer::Buffer, area: ratatui::layout::Rect) {
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buffer[(x, y)].reset();
                buffer[(x, y)].set_style(self.style());
            }
        }
    }
}
