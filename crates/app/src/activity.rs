//! The activity screen: a tile in the canvas (layout leaf `HarnessId::ACTIVITY`)
//! with the fleet's feed. Messages fold into openable bubbles; shared results
//! carry a link to their page.

use gpui::{AnyElement, Context, ElementId, FontWeight, div, prelude::*, px};
use termden_core::{Activity, HarnessId};

use crate::canvas::TRect;
use crate::panel::entry_color;
use crate::state::TermdenApp;
use crate::widgets::icons;

/// Messages, shared results and arrivals/departures; app and workspace
/// notices and status chatter stay out of the chat.
fn chat_worthy(entry: &Activity) -> bool {
    entry.detail.is_some()
        || entry.link.is_some()
        || (entry.harness.is_some()
            && (entry.message.starts_with("joined") || entry.message == "left"))
}

fn open_link(path: &str) {
    if let Some(base) = crate::choices::url()
        && let Ok(mut child) = std::process::Command::new("xdg-open")
            .arg(format!("{base}{path}"))
            .spawn()
    {
        std::thread::spawn(move || child.wait());
    }
}

pub fn render(app: &TermdenApp, rect: TRect, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let feed = div()
        .id("activity-feed")
        .track_scroll(&app.activity_scroll)
        .overflow_y_scroll()
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .p(px(12.))
        .children((!app.activity.iter().any(chat_worthy)).then(|| {
            div()
                .text_size(theme.ui(11.))
                .text_color(theme.overlay)
                .child("no messages yet")
        }))
        .children(
            app.activity
                .iter()
                .enumerate()
                .filter(|(_, entry)| chat_worthy(entry))
                .map(|(i, entry)| entry_row(app, i, entry, cx)),
        );

    crate::tile::screen_frame(
        app,
        HarnessId::ACTIVITY,
        rect,
        "Activity",
        icons::HISTORY,
        feed.into_any_element(),
        cx,
    )
}

fn entry_row(
    app: &TermdenApp,
    index: usize,
    entry: &Activity,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let theme = &app.theme;
    let by_name = |name: &str, stored: usize| {
        app.workspaces
            .iter()
            .flat_map(|w| &w.harnesses)
            .find(|h| h.name == name)
            .and_then(|h| h.color)
            .unwrap_or(stored)
    };
    let live = match entry.harness.and_then(|id| app.harness(id)) {
        Some((workspace, harness)) => harness.color.unwrap_or(workspace.accent),
        None => by_name(&entry.name, entry.accent),
    };
    let color = entry_color(theme, live);

    if entry.detail.is_none() && entry.link.is_none() {
        return div()
            .flex()
            .justify_center()
            .child(
                div()
                    .px(px(9.))
                    .py(px(2.))
                    .rounded_full()
                    .bg(theme.text.opacity(0.05))
                    .text_size(theme.ui(10.))
                    .text_color(theme.overlay)
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(div().text_color(color).child(entry.name.clone()))
                            .child(entry.message.clone())
                            .child(
                                div()
                                    .font_family(theme.mono_font.clone())
                                    .child(entry.time.clone()),
                            ),
                    ),
            )
            .into_any_element();
    }

    let sender = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(5.))
        .px(px(3.))
        .text_size(theme.ui(10.5))
        .child(div().size(px(7.)).rounded_full().bg(color))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(entry.name.clone()),
        )
        .children(entry.to.as_ref().map(|(to, accent)| {
            div()
                .flex()
                .gap(px(4.))
                .text_color(theme.overlay)
                .child("to")
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(entry_color(theme, by_name(to, *accent)))
                        .child(to.clone()),
                )
        }))
        .child(
            div()
                .font_family(theme.mono_font.clone())
                .text_size(theme.ui(9.))
                .text_color(theme.overlay.opacity(0.8))
                .child(entry.time.clone()),
        );

    let bubble = div()
        .id(ElementId::named_usize("activity-entry", index))
        .max_w(px(460.))
        .px(px(11.))
        .py(px(7.))
        .rounded_tl(px(3.))
        .rounded_tr(px(12.))
        .rounded_br(px(12.))
        .rounded_bl(px(12.))
        .bg(color.opacity(0.1))
        .border_1()
        .border_color(color.opacity(0.18))
        .text_size(theme.ui(11.5))
        .text_color(theme.text);

    let bubble = match (&entry.detail, &entry.link) {
        (Some(detail), _) => {
            let open = entry.open;
            bubble
                .cursor_pointer()
                .hover(|s| s.bg(color.opacity(0.16)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(entry) = this.activity.get_mut(index) {
                        entry.open = !entry.open;
                    }
                    cx.notify();
                }))
                .child(if open {
                    div().whitespace_normal().child(detail.clone())
                } else {
                    div().truncate().child(detail.clone())
                })
        }
        (None, Some(link)) => {
            let link = link.clone();
            bubble
                .flex()
                .flex_col()
                .items_start()
                .gap(px(5.))
                .child(div().child(entry.message.clone()))
                .child(
                    div()
                        .id(ElementId::named_usize("activity-link", index))
                        .px(px(8.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .bg(theme.blue.opacity(0.14))
                        .text_color(theme.blue)
                        .text_size(theme.ui(10.5))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.blue.opacity(0.26)))
                        .child("open results →")
                        .on_click(move |_, _, _| open_link(&link)),
                )
        }
        (None, None) => bubble,
    };

    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(3.))
        .child(sender)
        .child(bubble)
        .into_any_element()
}
