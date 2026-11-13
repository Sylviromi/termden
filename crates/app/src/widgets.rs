use gpui::{Div, ElementId, FontWeight, Hsla, SharedString, Stateful, div, prelude::*, px};
use termden_core::HarnessKind;

use crate::theme::Theme;

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Material Symbols Rounded, addressed by codepoint. Bundled as a subset
/// (assets/fonts): add a new icon's codepoint there too.
pub const ICON_FONT: &str = "Material Symbols Rounded";

pub mod icons {
    pub const SETTINGS: &str = "\u{e8b8}";
    pub const CLOSE: &str = "\u{e5cd}";
    pub const ADD: &str = "\u{e145}";
    pub const REMOVE: &str = "\u{e15b}";
    pub const CHECK: &str = "\u{e5ca}";
    pub const REFRESH: &str = "\u{e5d5}";
    pub const FULLSCREEN_EXIT: &str = "\u{e5d1}";
    pub const EXPAND_MORE: &str = "\u{e5cf}";
    pub const CHEVRON_RIGHT: &str = "\u{e5cc}";
    pub const CHEVRON_LEFT: &str = "\u{e5cb}";
    pub const TERMINAL: &str = "\u{eb8e}";
    pub const MINIMIZE: &str = "\u{e931}";
    pub const MAXIMIZE: &str = "\u{e3c1}";
    pub const RESTORE: &str = "\u{e3e0}";
}

pub fn hspacer() -> Div {
    div().flex_1()
}

pub fn dot(color: Hsla, size: f32) -> Div {
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded(px(size))
        .bg(color)
}

pub fn kind_badge(kind: HarnessKind, theme: &Theme) -> Div {
    let color = theme.kind(kind);
    div()
        .flex_shrink_0()
        .h(px(16.))
        .min_w(px(21.))
        .px(px(4.))
        .rounded(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .bg(color.opacity(0.16))
        .text_color(color)
        .font_family(theme.mono_font.clone())
        .text_size(theme.ui(8.5))
        .font_weight(FontWeight::SEMIBOLD)
        .child(kind.badge())
}

pub fn pill(text: impl Into<SharedString>, fg: Hsla, bg: Hsla, theme: &Theme) -> Div {
    div()
        .flex_shrink_0()
        .h(px(16.))
        .px(px(5.))
        .rounded(px(4.))
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .text_color(fg)
        .font_family(theme.mono_font.clone())
        .text_size(theme.ui(9.))
        .font_weight(FontWeight::MEDIUM)
        .child(text.into())
}

/// A harness tag ("FRONTEND", "TERMDEN DEV") as a colored badge. The color is
/// hashed from the tag text so the same tag always keeps the same accent.
pub fn tag_pill(tag: &str, theme: &Theme) -> Div {
    let color = theme.accent(
        tag.to_lowercase()
            .bytes()
            .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize)),
    );
    div()
        .flex_shrink_0()
        .h(px(16.))
        .px(px(5.))
        .rounded(px(4.))
        .flex()
        .items_center()
        .bg(color.opacity(0.18))
        .text_color(color)
        .font_family(theme.mono_font.clone())
        .text_size(theme.ui(9.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(tag.to_uppercase())
}

pub fn icon_button(id: impl Into<ElementId>, glyph: &'static str, theme: &Theme) -> Stateful<Div> {
    let hover_bg = theme.surface0;
    let hover_fg = theme.text;
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(22.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(theme.ui(12.))
        .text_color(theme.overlay)
        .cursor_pointer()
        .font_family(ICON_FONT)
        .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
        .child(glyph)
}

pub fn spinner_frame(tick: u64) -> &'static str {
    SPINNER[(tick as usize / 2) % SPINNER.len()]
}

/// Animated braille spinner. `tick` is bumped by the app's frame loop.
pub fn spinner(theme: &Theme, color: Hsla, tick: u64) -> Div {
    div()
        .flex_shrink_0()
        .text_color(color)
        .font_family(theme.mono_font.clone())
        .text_size(theme.ui(10.))
        .child(spinner_frame(tick))
}
