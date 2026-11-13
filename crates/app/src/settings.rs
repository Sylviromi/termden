//! The settings popup: theme, text size and the shortcut reference.

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};
use termden_core::{ThemeId, TitleBar};

use crate::panel::tip;
use crate::state::TermdenApp;
use crate::theme::Theme;
use crate::widgets::{ICON_FONT, icon_button, icons};

pub fn render(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    if !app.settings_open && !app.popups[crate::popup::SETTINGS].closing() {
        return None;
    }
    let theme = &app.theme;

    let card =
        div()
            .id("settings-card")
            .w(px(560.))
            .max_h(px(640.))
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
                (560., 640.),
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
                            .child(row("Theme", theme).child(theme_choices(app, cx)))
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
                            .child(tip("R", "reload termden (rebuilds reload too)", theme)),
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

fn theme_choices(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    div()
        .flex()
        .gap(px(10.))
        .children(ThemeId::ALL.into_iter().enumerate().map(|(index, id)| {
            let preview = Theme::for_id(id, &app.config);
            let selected = app.config.theme == id;
            let swatch = |color| div().size(px(14.)).rounded(px(7.)).bg(color);
            div()
                .id(("theme-choice", index))
                .flex()
                .flex_col()
                .gap(px(8.))
                .w(px(150.))
                .p(px(10.))
                .rounded(px(10.))
                .bg(preview.base)
                .border_2()
                .border_color(if selected {
                    theme.accent(0)
                } else {
                    theme.text.opacity(0.08)
                })
                .cursor_pointer()
                .hover(|style| style.opacity(0.85))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_theme(id);
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .gap(px(5.))
                        .child(swatch(preview.accent(0)))
                        .child(swatch(preview.accent(1)))
                        .child(swatch(preview.accent(2)))
                        .child(swatch(preview.accent(3)))
                        .child(swatch(preview.accent(4))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .text_size(theme.ui(10.5))
                                .text_color(preview.text)
                                .child(id.label()),
                        )
                        .children(selected.then(|| {
                            div()
                                .text_size(theme.ui(11.))
                                .text_color(preview.accent(0))
                                .font_family(ICON_FONT)
                                .child(icons::CHECK)
                        })),
                )
        }))
        .into_any_element()
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
