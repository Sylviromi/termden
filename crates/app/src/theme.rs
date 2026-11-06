use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor};
use gpui::{Hsla, Pixels, Rgba, SharedString, px, rgb};
use termden_core::{Config, HarnessKind, HarnessStatus, ThemeId};

/// A resolved theme: palette plus the font families picked in the config.
#[derive(Clone)]
pub struct Theme {
    pub crust: Hsla,
    pub mantle: Hsla,
    pub base: Hsla,
    pub surface0: Hsla,
    pub overlay: Hsla,
    pub subtext: Hsla,
    pub text: Hsla,

    pub red: Hsla,
    pub green: Hsla,
    pub yellow: Hsla,
    pub blue: Hsla,
    pub mauve: Hsla,
    pub peach: Hsla,

    pub accents: [Hsla; 8],
    /// The 16 ANSI terminal colors, normal then bright.
    pub ansi: [Hsla; 16],

    pub ui_font: SharedString,
    pub mono_font: SharedString,
    /// Terminal text size and the line (cell) height that goes with it.
    pub term_font: f32,
    pub term_line: f32,
    /// UI text scales with the configured font size (13px design baseline).
    pub ui_scale: f32,
}

impl Theme {
    pub fn for_id(id: ThemeId, config: &Config) -> Self {
        let palette = match id {
            ThemeId::CatppuccinMocha => catppuccin_mocha(),
            ThemeId::Everforest => everforest(),
        };

        Self {
            crust: palette.crust,
            mantle: palette.mantle,
            base: palette.base,
            surface0: palette.surface0,
            overlay: palette.overlay,
            subtext: palette.subtext,
            text: palette.text,
            red: palette.red,
            green: palette.green,
            yellow: palette.yellow,
            blue: palette.blue,
            mauve: palette.mauve,
            peach: palette.peach,
            accents: [
                palette.mauve,
                palette.blue,
                palette.green,
                palette.peach,
                palette.teal,
                palette.pink,
                palette.yellow,
                palette.red,
            ],
            ansi: [
                palette.surface0,
                palette.red,
                palette.green,
                palette.yellow,
                palette.blue,
                palette.mauve,
                palette.teal,
                palette.subtext,
                palette.overlay,
                palette.red,
                palette.green,
                palette.yellow,
                palette.blue,
                palette.pink,
                palette.teal,
                palette.text,
            ],
            ui_font: SharedString::from(config.ui_font.clone()),
            mono_font: SharedString::from(config.mono_font.clone()),
            term_font: config.font_size.clamp(8.0, 24.0),
            term_line: (config.font_size.clamp(8.0, 24.0) * 19.0 / 12.0).round(),
            ui_scale: config.font_size.clamp(8.0, 24.0) / 13.0,
        }
    }

    pub fn ui(&self, size: f32) -> Pixels {
        px(size * self.ui_scale)
    }

    pub fn accent(&self, index: usize) -> Hsla {
        self.accents[index % self.accents.len()]
    }

    pub fn status(&self, status: HarnessStatus) -> Hsla {
        match status {
            HarnessStatus::Working => self.green,
            HarnessStatus::Idle => self.overlay,
            HarnessStatus::NeedsInput => self.blue,
            HarnessStatus::Error => self.red,
        }
    }

    /// Resolve a terminal cell color. `None` means "the default for this
    /// slot" (theme text for foreground, tile background for background).
    pub fn term_color(&self, color: TermColor) -> Option<Hsla> {
        match color {
            TermColor::Spec(c) => Some(rgb8(c.r, c.g, c.b)),
            TermColor::Indexed(i) => Some(self.indexed(i)),
            TermColor::Named(name) => {
                let i = name as usize;
                if i < 16 {
                    Some(self.ansi[i])
                } else if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize)
                    .contains(&i)
                {
                    Some(self.ansi[i - NamedColor::DimBlack as usize].opacity(0.7))
                } else {
                    None
                }
            }
        }
    }

    /// xterm 256-color table: 16 themed colors, a 6x6x6 cube, 24 grays.
    fn indexed(&self, i: u8) -> Hsla {
        match i {
            0..16 => self.ansi[i as usize],
            16..232 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                rgb8(level(i / 36), level(i / 6 % 6), level(i % 6))
            }
            _ => {
                let v = 8 + 10 * (i - 232);
                rgb8(v, v, v)
            }
        }
    }

    pub fn kind(&self, kind: HarnessKind) -> Hsla {
        match kind {
            HarnessKind::ClaudeCode => self.peach,
            HarnessKind::OpenCode => self.green,
            HarnessKind::Pi => self.mauve,
            HarnessKind::Shell => self.blue,
        }
    }
}

struct Palette {
    crust: Hsla,
    mantle: Hsla,
    base: Hsla,
    surface0: Hsla,
    overlay: Hsla,
    subtext: Hsla,
    text: Hsla,
    red: Hsla,
    green: Hsla,
    yellow: Hsla,
    blue: Hsla,
    mauve: Hsla,
    peach: Hsla,
    teal: Hsla,
    pink: Hsla,
}

fn catppuccin_mocha() -> Palette {
    Palette {
        crust: c(0x11111b),
        mantle: c(0x181825),
        base: c(0x1e1e2e),
        surface0: c(0x313244),
        overlay: c(0x6c7086),
        subtext: c(0xa6adc8),
        text: c(0xcdd6f4),
        red: c(0xf38ba8),
        green: c(0xa6e3a1),
        yellow: c(0xf9e2af),
        blue: c(0x89b4fa),
        mauve: c(0xcba6f7),
        peach: c(0xfab387),
        teal: c(0x94e2d5),
        pink: c(0xf5c2e7),
    }
}

fn everforest() -> Palette {
    Palette {
        crust: c(0x1e2326),
        mantle: c(0x272e33),
        base: c(0x2d353b),
        surface0: c(0x343f44),
        overlay: c(0x859289),
        subtext: c(0x9da9a0),
        text: c(0xd3c6aa),
        red: c(0xe67e80),
        green: c(0xa7c080),
        yellow: c(0xdbbc7f),
        blue: c(0x7fbbb3),
        mauve: c(0xd699b6),
        peach: c(0xe69875),
        teal: c(0x83c092),
        pink: c(0xd699b6),
    }
}

fn rgb8(r: u8, g: u8, b: u8) -> Hsla {
    c(u32::from_be_bytes([0, r, g, b]))
}

fn c(hex: u32) -> Hsla {
    Rgba::from(rgb(hex)).into()
}
