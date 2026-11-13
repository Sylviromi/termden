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
    // The theme panel opens attached to the card's right edge.
    let panel_w = 330.0;
    let width = 560.0
        + if app.theme_browser.is_some() {
            panel_w
        } else {
            0.0
        };

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
                (width, height),
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
                            .child(row("Theme", theme).child(current_theme(app, cx)))
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

    let card = match &app.theme_browser {
        Some(browser) => div()
            .id("settings-wrap")
            .flex()
            .h(px(height))
            .child(card.h_full().rounded_tr(px(0.)).rounded_br(px(0.)))
            .child(theme_panel(app, browser, panel_w, cx)),
        None => card,
    };
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

/// The open theme panel: what has been typed into its search bar.
pub struct ThemeBrowser {
    pub filter: String,
}

/// Position of theme `id` among the panel list's children, which put a
/// DARK / LIGHT heading before each group (for `ScrollHandle::scroll_to_item`).
pub fn panel_index(filter: &str, id: &str) -> Option<usize> {
    let list = crate::theme::browse(filter);
    let i = list.iter().position(|t| t.id == id)?;
    let headings = if list[i].dark {
        1
    } else {
        1 + usize::from(list.iter().any(|t| t.dark))
    };
    Some(i + headings)
}

/// The current theme as a card; clicking it opens (or closes) the panel.
fn current_theme(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let def = crate::theme::theme_def(&app.config.theme.0);
    let open = app.theme_browser.is_some();
    theme_card(app, usize::MAX, def.id, def.name, open)
        .on_click(cx.listener(|this, _, _, cx| {
            this.theme_browser = match this.theme_browser {
                Some(_) => None,
                None => {
                    // Open on the current theme, wherever it is in the list.
                    this.theme_scroll_to
                        .set(panel_index("", &this.config.theme.0));
                    Some(ThemeBrowser {
                        filter: String::new(),
                    })
                }
            };
            cx.notify();
        }))
        .into_any_element()
}

/// A theme in its own colours: background, five accent dots, its name.
fn theme_card(
    app: &TermdenApp,
    index: usize,
    id: &'static str,
    name: &'static str,
    selected: bool,
) -> gpui::Stateful<gpui::Div> {
    let theme = &app.theme;
    let preview = Theme::for_id(id, &app.config);
    let dot = |color| div().size(px(10.)).rounded(px(5.)).bg(color);
    div()
        .id(("theme-card", index))
        .w(px(142.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .p(px(9.))
        .rounded(px(9.))
        .bg(preview.base)
        .border_2()
        .border_color(if selected {
            theme.accent(0)
        } else {
            preview.text.opacity(0.10)
        })
        .cursor_pointer()
        .hover(|style| style.opacity(0.85))
        .child(
            div()
                .flex()
                .gap(px(4.))
                .child(dot(preview.accent(0)))
                .child(dot(preview.accent(1)))
                .child(dot(preview.accent(2)))
                .child(dot(preview.accent(3)))
                .child(dot(preview.accent(4))),
        )
        .child(
            div()
                .truncate()
                .text_size(theme.ui(10.))
                .text_color(preview.text)
                .child(name),
        )
}

/// Side panel attached to the settings card: a search bar and every theme
/// (Dark, then Light) as cards. Typing filters; arrows preview; Enter keeps.
fn theme_panel(
    app: &TermdenApp,
    browser: &ThemeBrowser,
    width: f32,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let theme = &app.theme;
    let current = crate::theme::theme_def(&app.config.theme.0).id;
    let list = crate::theme::browse(&browser.filter);
    if let Some(row) = app.theme_scroll_to.get() {
        if app.theme_scroll.bounds_for_item(row).is_some() {
            app.theme_scroll.scroll_to_item(row);
            app.theme_scroll_to.set(None);
        } else {
            cx.notify(); // not laid out yet: try again next frame
        }
    }
    let header = |text: &'static str| {
        div()
            .w_full()
            .pt(px(6.))
            .text_size(theme.ui(9.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.overlay)
            .child(text)
    };
    // Headers and cards are siblings so scroll_to_item can reach each card.
    let mut items: Vec<AnyElement> = Vec::new();
    for dark in [true, false] {
        let group: Vec<_> = list.iter().filter(|t| t.dark == dark).collect();
        if group.is_empty() {
            continue;
        }
        items.push(header(if dark { "DARK" } else { "LIGHT" }).into_any_element());
        for def in group {
            let index = crate::theme::all()
                .iter()
                .position(|t| t.id == def.id)
                .unwrap_or(0);
            let id = def.id;
            items.push(
                theme_card(app, index, def.id, def.name, def.id == current)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_theme(id);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
    }
    let empty = items.is_empty();
    div()
        .w(px(width))
        .h_full()
        .flex()
        .flex_col()
        .bg(theme.mantle)
        .border_1()
        .border_l_0()
        .border_color(theme.text.opacity(0.10))
        .rounded_tr(px(14.))
        .rounded_br(px(14.))
        .shadow_lg()
        .child(
            div()
                .flex_shrink_0()
                .p(px(14.))
                .border_b_1()
                .border_color(theme.text.opacity(0.06))
                .child(
                    div()
                        .h(px(32.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(8.))
                        .bg(theme.crust)
                        .border_1()
                        .border_color(theme.accent(0).opacity(0.5))
                        .child(
                            div()
                                .font_family(crate::widgets::ICON_FONT)
                                .text_size(theme.ui(14.))
                                .text_color(theme.overlay)
                                .child(icons::SEARCH),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(theme.ui(11.))
                                .text_color(if browser.filter.is_empty() {
                                    theme.overlay
                                } else {
                                    theme.text
                                })
                                .child(if browser.filter.is_empty() {
                                    "Search themes…".to_string()
                                } else {
                                    format!("{}▏", browser.filter)
                                }),
                        ),
                ),
        )
        .child(
            div()
                .id("theme-list")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(&app.theme_scroll)
                .p(px(14.))
                .flex()
                .flex_wrap()
                .content_start()
                .gap(px(8.))
                .children(items)
                .when(empty, |d| {
                    d.child(
                        div()
                            .text_size(theme.ui(10.5))
                            .text_color(theme.overlay)
                            .child("No theme matches"),
                    )
                }),
        )
        .child(
            div()
                .flex_shrink_0()
                .px(px(14.))
                .py(px(8.))
                .border_t_1()
                .border_color(theme.text.opacity(0.06))
                .text_size(theme.ui(9.5))
                .text_color(theme.overlay)
                .child("Type to search · ↑↓ preview · Enter keep · Esc close"),
        )
        .into_any_element()
}
