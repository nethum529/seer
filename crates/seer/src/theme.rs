use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders};

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) accent: Color,
    pub(crate) blue: Color,
    pub(crate) panel_bg: Color,
    pub(crate) surface0: Color,
    pub(crate) surface1: Color,
    pub(crate) overlay0: Color,
    pub(crate) text: Color,
    pub(crate) subtext0: Color,
    pub(crate) mauve: Color,
    pub(crate) green: Color,
    pub(crate) yellow: Color,
    pub(crate) red: Color,
    pub(crate) teal: Color,
    pub(crate) peach: Color,
}

impl Default for Palette {
    fn default() -> Self {
        if std::env::var("COLORTERM").is_ok_and(|value| value == "truecolor") {
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
            panel_bg: Color::Rgb(24, 24, 37),
            surface0: Color::Rgb(49, 50, 68),
            surface1: Color::Rgb(69, 71, 90),
            overlay0: Color::Rgb(108, 112, 134),
            text: Color::Rgb(205, 214, 244),
            subtext0: Color::Rgb(166, 173, 200),
            mauve: Color::Rgb(203, 166, 247),
            green: Color::Rgb(166, 227, 161),
            yellow: Color::Rgb(249, 226, 175),
            red: Color::Rgb(243, 139, 168),
            teal: Color::Rgb(148, 226, 213),
            peach: Color::Rgb(250, 179, 135),
        }
    }

    pub(crate) fn terminal() -> Self {
        Self {
            accent: Color::LightBlue,
            blue: Color::LightBlue,
            panel_bg: Color::Black,
            surface0: Color::DarkGray,
            surface1: Color::DarkGray,
            overlay0: Color::Gray,
            text: Color::White,
            subtext0: Color::Gray,
            mauve: Color::LightMagenta,
            green: Color::LightGreen,
            yellow: Color::LightYellow,
            red: Color::LightRed,
            teal: Color::LightCyan,
            peach: Color::Yellow,
        }
    }

    pub(crate) fn style(self) -> Style {
        Style::default().fg(self.text).bg(self.panel_bg)
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

    pub(crate) fn ansi(self, index: u8) -> Color {
        let colors = [
            self.panel_bg,
            self.red,
            self.green,
            self.yellow,
            self.blue,
            self.mauve,
            self.teal,
            self.surface1,
            self.surface0,
            self.red,
            self.green,
            self.peach,
            self.blue,
            self.mauve,
            self.teal,
            self.text,
        ];
        colors[usize::from(index % 16)]
    }
}
