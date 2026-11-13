use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor};
use gpui::{Hsla, Pixels, Rgba, SharedString, px, rgb};
use termden_core::{Config, HarnessKind, HarnessStatus};

use crate::themes::{THEMES, ThemeDef};

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
    /// Terminal cursor block.
    pub cursor: Hsla,
    /// Dark background; light themes need darker accents for text.
    pub dark: bool,

    pub ui_font: SharedString,
    pub mono_font: SharedString,
    /// Terminal text size and the line (cell) height that goes with it.
    pub term_font: f32,
    pub term_line: f32,
    /// UI text scales with the configured font size (13px design baseline).
    pub ui_scale: f32,
}

impl Theme {
    /// The theme with `id`, or the first (default) one if it is unknown.
    pub fn for_id(id: &str, config: &Config) -> Self {
        let def = theme_def(id);
        // The two original themes keep their hand-tuned UI colors exactly.
        let (palette, ansi) = match def.id {
            "catppuccin-mocha" => (catppuccin_mocha(), None),
            "everforest" => (everforest(), None),
            _ => (derive(def), Some(def.ansi.map(c))),
        };
        let cursor = if ansi.is_some() {
            c(def.cursor)
        } else {
            palette.text
        };
        let ansi = ansi.unwrap_or([
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
        ]);

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
            ansi,
            cursor,
            dark: def.dark,
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

    /// An accent pushed toward pastel-neon: saturated, light on dark
    /// themes and deep on light ones so names stay readable.
    pub fn agent(&self, index: usize) -> Hsla {
        let c = self.accent(index);
        Hsla {
            s: (c.s * 1.15).clamp(0.8, 1.0),
            l: if self.dark { 0.76 } else { 0.38 },
            ..c
        }
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

/// All themes, in settings order.
pub fn all() -> &'static [ThemeDef] {
    &THEMES
}

pub fn theme_def(id: &str) -> &'static ThemeDef {
    THEMES.iter().find(|t| t.id == id).unwrap_or(&THEMES[0])
}

/// The theme after `id` (wrapping), for ctrl+shift+t-style cycling.
pub fn next_id(id: &str) -> &'static str {
    let i = THEMES.iter().position(|t| t.id == id).unwrap_or(0);
    THEMES[(i + 1) % THEMES.len()].id
}

/// UI palette from a published terminal theme: background shades mixed
/// toward black (dark) or the text color (light), surfaces and secondary
/// text mixed between background and text, accents from the ANSI colors.
fn derive(def: &ThemeDef) -> Palette {
    let bg = c(def.bg);
    let fg = c(def.fg);
    let a = def.ansi.map(c);
    let (mantle, crust) = if def.dark {
        (mix(bg, c(0x000000), 0.18), mix(bg, c(0x000000), 0.35))
    } else {
        (mix(bg, fg, 0.05), mix(bg, fg, 0.10))
    };
    let accent = |color| readable(color, bg, def.dark, 3.0);
    // Body text gets the stricter WCAG AA bar (Solarized Light misses it).
    let fg = readable(fg, bg, def.dark, 4.5);
    Palette {
        crust,
        mantle,
        base: bg,
        surface0: mix(bg, fg, 0.12),
        overlay: mix(bg, fg, 0.42),
        subtext: mix(bg, fg, 0.78),
        text: fg,
        red: accent(a[1]),
        green: accent(a[2]),
        yellow: accent(a[3]),
        blue: accent(a[4]),
        mauve: accent(a[5]),
        peach: accent(mix(a[1], a[3], 0.5)),
        teal: accent(a[6]),
        pink: accent(a[13]),
    }
}

/// The theme before `id` (wrapping).
pub fn prev_id(id: &str) -> &'static str {
    let i = THEMES.iter().position(|t| t.id == id).unwrap_or(0);
    THEMES[(i + THEMES.len() - 1) % THEMES.len()].id
}

/// Themes the settings browser lists: the Dark or Light tab, or, while a
/// filter is typed, every theme whose name contains it.
pub fn browse(dark: bool, filter: &str) -> Vec<&'static ThemeDef> {
    let filter = filter.to_lowercase();
    THEMES
        .iter()
        .filter(|t| {
            if filter.is_empty() {
                t.dark == dark
            } else {
                t.name.to_lowercase().contains(&filter) || t.id.contains(&filter)
            }
        })
        .collect()
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    let m = |x: f32, y: f32| x + (y - x) * t;
    Rgba {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
        a: 1.0,
    }
    .into()
}

/// WCAG relative luminance.
fn luminance(color: Hsla) -> f32 {
    let c = Rgba::from(color);
    let lin = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Push `color` lighter (dark themes) or darker (light themes) until its
/// contrast on `bg` reaches `min`: some palettes have e.g. a pale yellow on
/// a white page.
fn readable(color: Hsla, bg: Hsla, dark: bool, min: f32) -> Hsla {
    let mut color = color;
    for _ in 0..40 {
        if contrast(color, bg) >= min {
            break;
        }
        color.l = (color.l + if dark { 0.02 } else { -0.02 }).clamp(0.0, 1.0);
    }
    color
}

fn rgb8(r: u8, g: u8, b: u8) -> Hsla {
    c(u32::from_be_bytes([0, r, g, b]))
}

fn c(hex: u32) -> Hsla {
    Rgba::from(rgb(hex)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_is_usable() {
        let config = Config::default();
        let mut ids = std::collections::HashSet::new();
        for def in all() {
            assert!(ids.insert(def.id), "duplicate theme id {}", def.id);
            let theme = Theme::for_id(def.id, &config);
            // Accents are used as text (badges, names): they must read on base.
            for accent in theme.accents {
                assert!(
                    contrast(accent, theme.base) >= 2.9,
                    "{}: accent {accent:?} unreadable on base",
                    def.id
                );
            }
            assert!(contrast(theme.text, theme.base) >= 4.5, "{}: text", def.id);
        }
        assert_eq!(theme_def("no-such-theme").id, "catppuccin-mocha");
        let mut id = all()[0].id;
        for _ in 0..all().len() {
            id = next_id(id);
        }
        assert_eq!(id, all()[0].id, "cycling visits every theme once");
        assert_eq!(prev_id(next_id("nord")), "nord");
    }

    #[test]
    fn browse_filters_by_tab_then_by_name() {
        assert!(browse(true, "").iter().all(|t| t.dark));
        assert!(browse(false, "").iter().all(|t| !t.dark));
        // A filter searches both tabs, by name or id.
        let gruvbox: Vec<_> = browse(true, "GRU").iter().map(|t| t.id).collect();
        assert!(gruvbox.contains(&"gruvbox-dark") && gruvbox.contains(&"gruvbox-light"));
        assert!(browse(true, "zzz").is_empty());
    }
}
