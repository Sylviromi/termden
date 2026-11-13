use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which built-in palette to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeId {
    CatppuccinMocha,
    Everforest,
}

impl Default for ThemeId {
    fn default() -> Self {
        Self::CatppuccinMocha
    }
}

impl ThemeId {
    pub const ALL: [ThemeId; 2] = [ThemeId::CatppuccinMocha, ThemeId::Everforest];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::CatppuccinMocha => "Catppuccin Mocha",
            ThemeId::Everforest => "Everforest",
        }
    }

    pub fn next(self) -> Self {
        match self {
            ThemeId::CatppuccinMocha => ThemeId::Everforest,
            ThemeId::Everforest => ThemeId::CatppuccinMocha,
        }
    }
}

/// Window title bar. `Auto` hides it under tiling window managers and asks
/// the desktop for its own everywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleBar {
    #[default]
    Auto,
    System,
    None,
}

impl TitleBar {
    pub const ALL: [TitleBar; 3] = [TitleBar::Auto, TitleBar::System, TitleBar::None];

    pub fn label(self) -> &'static str {
        match self {
            TitleBar::Auto => "Auto",
            TitleBar::System => "System",
            TitleBar::None => "None",
        }
    }

    /// Whether a title bar is wanted, resolving `Auto` from the environment.
    pub fn wanted(self) -> bool {
        match self {
            TitleBar::System => true,
            TitleBar::None => false,
            TitleBar::Auto => !tiling_wm(),
        }
    }
}

/// Tiling window managers place and size windows themselves; a title bar
/// there is only clutter.
fn tiling_wm() -> bool {
    [
        "HYPRLAND_INSTANCE_SIGNATURE",
        "SWAYSOCK",
        "I3SOCK",
        "NIRI_SOCKET",
    ]
    .iter()
    .any(|var| std::env::var_os(var).is_some_and(|v| !v.is_empty()))
}

/// User configuration, read from `$XDG_CONFIG_HOME/termden/config.toml`.
///
/// Every field has a default so the file can stay tiny.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeId,
    pub ui_font: String,
    pub mono_font: String,
    /// Terminal text size in px (ctrl+shift +/- adjusts and saves it).
    pub font_size: f32,
    pub panel_width: f32,
    pub gap: f32,
    pub corner_radius: f32,
    pub title_bar: TitleBar,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeId::CatppuccinMocha,
            ui_font: "Noto Sans".to_string(),
            mono_font: "JetBrainsMono Nerd Font".to_string(),
            font_size: 16.0,
            panel_width: 268.0,
            gap: 12.0,
            corner_radius: 12.0,
            title_bar: TitleBar::Auto,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        config_dir().join("termden").join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };

        match toml::from_str(&text) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("termden: could not parse {}: {error}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, text)
    }
}

fn config_dir() -> PathBuf {
    crate::state::xdg_dir("XDG_CONFIG_HOME", ".config")
}
