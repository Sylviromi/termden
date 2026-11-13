//! The settings popup: theme, text size and the shortcut reference.

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};
use termden_core::TitleBar;

use crate::panel::tip;
use crate::state::TermdenApp;
use crate::theme::Theme;
use crate::widgets::{icon_button, icons};

pub fn render(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    if !app.settings_open && !app.popups[crate::popup::SETTINGS].closing() {
        return None;
    }
    let theme = &app.theme;
    // Never taller than the window (the theme list makes it long).
    let height = (app.viewport.1 - 48.0).min(640.0);

    let card =
        div()
            .id("settings-card")
            .w(px(560.))
            .max_h(px(height))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(theme.mantle)
            .border_1()
            .border_color(theme.text.opacity(0.10))
            .shadow_lg()
            .overflow_hidden()
            .child(crate::popup::drag_handle(
                div()
                    .flex()
                    .items_center()
                    .h(px(54.))
                    .px(px(18.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(theme.text.opacity(0.06))
                    .child(
                        div()
                            .flex_1()
                            .text_size(theme.ui(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Settings"),
                    )
                    .child(
                        div()
                            .mr(px(10.))
                            .text_size(theme.ui(9.5))
                            .text_color(theme.overlay)
                            .child("Esc to close"),
                    )
                    .child(icon_button("settings-close", icons::CLOSE, theme).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.settings_open = false;
                            cx.notify();
                        }),
                    )),
                crate::popup::SETTINGS,
                (560., height),
                cx,
            ))
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(20.))
                    .child(section(
                        "APPEARANCE",
                        theme,
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(row("Theme", theme).child(theme_stepper(app, cx)))
                            .children(
                                app.theme_browser
                                    .as_ref()
                                    .map(|b| theme_browser(app, b, cx)),
                            )
                            .child(row("Text size", theme).child(font_stepper(app, cx)))
                            .child(row("Title bar", theme).child(title_bar_choices(app, cx))),
                    ))
                    .child(section(
                        "SHORTCUTS · CTRL+SHIFT +",
                        theme,
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(9.))
                            .child(tip("T", "new shell · agents via +", theme))
                            .child(tip("←→", "move focus between tiles (or hjkl)", theme))
                            .child(tip("Z", "zoom tile · double-click its header", theme))
                            .child(tip("W", "close tile (press twice)", theme))
                            .child(tip("PgUp", "previous / next workspace (PgDn)", theme))
                            .child(tip("V", "paste · drag to select and copy", theme))
                            .child(tip("+ −", "terminal text size (0 resets)", theme))
                            .child(tip(",", "open these settings", theme))
                            .child(tip("R", "restart termden (e.g. into a new build)", theme))
                            .child(tip("F11", "fullscreen (no ctrl+shift)", theme)),
                    )),
            );

    Some(crate::popup::frame(
        "settings-backdrop",
        crate::popup::SETTINGS,
        app,
        card,
        |this| this.settings_open = false,
        cx,
    ))
}

fn section(title: &'static str, theme: &Theme, body: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .text_size(theme.ui(9.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.overlay)
                .child(title),
        )
        .child(
            div()
                .rounded(px(10.))
                .border_1()
                .border_color(theme.text.opacity(0.06))
                .bg(theme.text.opacity(0.02))
                .p(px(14.))
                .child(body),
        )
}

fn row(label: &'static str, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
        .child(
            div()
                .text_size(theme.ui(11.))
                .text_color(theme.text)
                .child(label),
        )
}

fn font_stepper(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let size = app.config.font_size;
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(
            icon_button("font-dec", icons::REMOVE, theme).on_click(cx.listener(
                |this, _, _, cx| {
                    this.set_font_size(this.config.font_size - 1.0);
                    cx.notify();
                },
            )),
        )
        .child(
            div()
                .min_w(px(46.))
                .flex()
                .justify_center()
                .font_family(theme.mono_font.clone())
                .text_size(theme.ui(11.))
                .text_color(theme.text)
                .child(format!("{size:.0} px")),
        )
        .child(
            icon_button("font-inc", icons::ADD, theme).on_click(cx.listener(|this, _, _, cx| {
                this.set_font_size(this.config.font_size + 1.0);
                cx.notify();
            })),
        )
        .into_any_element()
}

/// Auto / System / None; applies to the open window right away.
fn title_bar_choices(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    div()
        .flex()
        .gap(px(6.))
        .children(
            TitleBar::ALL
                .into_iter()
                .enumerate()
                .map(|(index, choice)| {
                    let selected = app.config.title_bar == choice;
                    div()
                        .id(("title-bar-choice", index))
                        .px(px(10.))
                        .h(px(26.))
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(if selected {
                            theme.accent(0)
                        } else {
                            theme.text.opacity(0.10)
                        })
                        .bg(if selected {
                            theme.accent(0).opacity(0.14)
                        } else {
                            theme.text.opacity(0.0)
                        })
                        .text_size(theme.ui(10.5))
                        .text_color(if selected { theme.text } else { theme.subtext })
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.surface0))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_title_bar(choice, window);
                            cx.notify();
                        }))
                        .child(choice.label())
                }),
        )
        .into_any_element()
}

/// Theme chooser state: which tab is shown and what has been typed.
pub struct ThemeBrowser {
    pub dark: bool,
    pub filter: String,
}

/// `‹ Current theme ›  Browse`: arrows step through every theme live.
fn theme_stepper(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let current = crate::theme::theme_def(&app.config.theme.0);
    let open = app.theme_browser.is_some();
    let step = |id: &'static str, icon, prev: bool, cx: &mut Context<TermdenApp>| {
        icon_button(id, icon, theme)
            .size(px(24.))
            .text_size(theme.ui(14.))
            .on_click(cx.listener(move |this, _, _, cx| {
                let id = &this.config.theme.0;
                let next = if prev {
                    crate::theme::prev_id(id)
                } else {
                    crate::theme::next_id(id)
                };
                this.set_theme(next);
                cx.notify();
            }))
    };
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .child(step("theme-prev", icons::CHEVRON_LEFT, true, cx))
        .child(
            div()
                .w(px(150.))
                .flex()
                .justify_center()
                .truncate()
                .text_size(theme.ui(11.))
                .text_color(theme.text)
                .child(current.name),
        )
        .child(step("theme-next", icons::CHEVRON_RIGHT, false, cx))
        .child(
            div()
                .id("theme-browse")
                .ml(px(6.))
                .px(px(10.))
                .h(px(26.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .border_1()
                .border_color(if open {
                    theme.accent(0)
                } else {
                    theme.text.opacity(0.10)
                })
                .text_size(theme.ui(10.5))
                .text_color(theme.subtext)
                .cursor_pointer()
                .hover(|style| style.bg(theme.surface0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.theme_browser = match this.theme_browser {
                        Some(_) => None,
                        None => Some(ThemeBrowser {
                            dark: crate::theme::theme_def(&this.config.theme.0).dark,
                            filter: String::new(),
                        }),
                    };
                    cx.notify();
                }))
                .child(if open { "Close" } else { "Browse" }),
        )
        .into_any_element()
}

/// Dark/Light tabs, the typed filter, and a short scrolling list of themes.
fn theme_browser(
    app: &TermdenApp,
    browser: &ThemeBrowser,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let theme = &app.theme;
    let list = crate::theme::browse(browser.dark, &browser.filter);
    let filtering = !browser.filter.is_empty();
    let tab = |dark: bool, cx: &mut Context<TermdenApp>| {
        let active = !filtering && browser.dark == dark;
        let count = crate::theme::all()
            .iter()
            .filter(|t| t.dark == dark)
            .count();
        div()
            .id(if dark { "tab-dark" } else { "tab-light" })
            .px(px(10.))
            .h(px(24.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .text_size(theme.ui(10.))
            .text_color(if active { theme.text } else { theme.overlay })
            .when(active, |d| d.bg(theme.surface0))
            .cursor_pointer()
            .hover(|style| style.text_color(theme.text))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(b) = this.theme_browser.as_mut() {
                    b.dark = dark;
                    b.filter.clear();
                }
                cx.notify();
            }))
            .child(format!("{} {count}", if dark { "Dark" } else { "Light" }))
    };
    let rows: Vec<_> = list
        .iter()
        .enumerate()
        .map(|(i, def)| {
            let preview = Theme::for_id(def.id, &app.config);
            let selected = crate::theme::theme_def(&app.config.theme.0).id == def.id;
            let id = def.id;
            let dot = |color| div().size(px(8.)).rounded(px(4.)).bg(color);
            div()
                .id(("theme-row", i))
                .flex_shrink_0()
                .h(px(30.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(7.))
                .border_1()
                .border_color(if selected {
                    theme.accent(0)
                } else {
                    theme.text.opacity(0.0)
                })
                .when(selected, |d| d.bg(theme.accent(0).opacity(0.10)))
                .cursor_pointer()
                .hover(|style| style.bg(theme.surface0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_theme(id);
                    cx.notify();
                }))
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .px(px(6.))
                        .h(px(18.))
                        .rounded(px(5.))
                        .bg(preview.base)
                        .border_1()
                        .border_color(preview.text.opacity(0.15))
                        .child(dot(preview.accent(0)))
                        .child(dot(preview.accent(1)))
                        .child(dot(preview.accent(2)))
                        .child(dot(preview.accent(3))),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_size(theme.ui(10.5))
                        .text_color(theme.text)
                        .child(def.name),
                )
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(10.))
        .rounded(px(9.))
        .bg(theme.crust.opacity(0.5))
        .border_1()
        .border_color(theme.text.opacity(0.06))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(tab(true, cx))
                .child(tab(false, cx))
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(theme.ui(10.))
                        .text_color(if filtering { theme.text } else { theme.overlay })
                        .child(if filtering {
                            format!("filter: {}▏", browser.filter)
                        } else {
                            "type to filter".into()
                        }),
                ),
        )
        .child(
            div()
                .id("theme-list")
                .max_h(px(216.))
                .overflow_y_scroll()
                .track_scroll(&app.theme_scroll)
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(rows)
                .when(list.is_empty(), |d| {
                    d.child(
                        div()
                            .p(px(8.))
                            .text_size(theme.ui(10.5))
                            .text_color(theme.overlay)
                            .child("No theme matches"),
                    )
                }),
        )
        .child(
            div()
                .text_size(theme.ui(9.5))
                .text_color(theme.overlay)
                .child("↑↓ preview · Enter keep · Tab dark/light · Esc close"),
        )
        .into_any_element()
}
