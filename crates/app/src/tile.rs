use gpui::{
    AnyElement, Context, ElementId, FontWeight, Hsla, MouseButton, MouseDownEvent,
    ScrollWheelEvent, div, prelude::*, px,
};
use termden_core::{DropZone, HarnessId};

use crate::canvas::TRect;
use crate::state::TermdenApp;
use crate::terminal;
use crate::theme::Theme;
use crate::widgets::{dot, icon_button, icons, kind_badge, pill, spinner, tag_pill};

const BORDER: f32 = 2.0;
const HEADER: f32 = 33.0; // 32 + 1px rule
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 8.0;

/// Window position of the first terminal cell of a tile. The grid is
/// bottom-anchored, so the leftover fraction of a row sits above it.
pub fn content_origin(rect: TRect, rows: u16, cell_h: f32) -> (f32, f32) {
    let area = rect.h - 2.0 * (BORDER + PAD_Y) - HEADER;
    let slack = (area - f32::from(rows) * cell_h).max(0.0);
    (
        rect.x + BORDER + PAD_X,
        rect.y + BORDER + HEADER + PAD_Y + slack,
    )
}

pub fn render(
    app: &TermdenApp,
    id: HarnessId,
    rect: TRect,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let Some((workspace, harness)) = app.harness(id) else {
        return div().into_any_element();
    };

    let theme = &app.theme;
    let accent = theme.accent(workspace.accent);
    let name_color = theme.agent(harness.color.unwrap_or(workspace.accent));
    let focused = app.focused == Some(id);
    let dragging = app
        .drag
        .is_some_and(|drag| drag.active && drag.harness == id);
    let drop_zone = app
        .drop_target
        .filter(|(target, _)| *target == id)
        .map(|(_, zone)| zone);

    let session = app.sessions.get(&id);
    let exited = session.is_some_and(|s| s.exited);
    let zoomed = app.zoomed == Some(id);
    let close_armed = app.close_armed(id);

    let mut tile = div()
        .id(ElementId::named_usize("tile", id.raw() as usize))
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.w))
        .h(px(rect.h))
        .rounded(px(10.))
        .bg(theme.base)
        .border_2()
        .border_color(if focused {
            accent
        } else {
            theme.text.opacity(0.06)
        })
        .overflow_hidden()
        .flex()
        .flex_col()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| {
                window.focus(&this.focus);
                this.focus_harness(id);
                cx.notify();
            }),
        );

    if focused {
        tile = tile.shadow_lg();
    }
    if dragging {
        tile = tile.opacity(0.55);
    }

    tile.child(
        div()
            .id(ElementId::named_usize("tile-header", id.raw() as usize))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(7.))
            .h(px(32.))
            .px(px(9.))
            .bg(theme.surface0.opacity(0.26))
            .border_b_1()
            .border_color(theme.text.opacity(0.05))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 2 {
                        this.focus_harness(id);
                        this.toggle_zoom();
                        cx.notify();
                    } else {
                        this.begin_drag(id, event.position);
                    }
                }),
            )
            .child(dot(theme.status(harness.status), 9.))
            .child(if crate::panel::pending(harness) {
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(5.))
                    .child(
                        spinner(theme, theme.overlay, app.tick)
                            .relative()
                            .top(px(2.)),
                    )
                    .child(
                        div()
                            .text_size(theme.ui(11.5))
                            .text_color(theme.overlay)
                            .child("starting…"),
                    )
                    .into_any_element()
            } else {
                div()
                    .text_size(theme.ui(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(name_color)
                    .child(harness.name.clone())
                    .into_any_element()
            })
            .children((!harness.pronouns.is_empty()).then(|| {
                div()
                    .text_size(theme.ui(10.))
                    .text_color(theme.overlay)
                    .child(harness.pronouns.clone())
            }))
            .child(kind_badge(harness.kind, theme))
            .children((!harness.tag.is_empty()).then(|| tag_pill(&harness.tag, theme)))
            .child(div().flex_1().min_w(px(0.)))
            .child(status_chips(
                harness,
                app.sessions.get(&id).map(|s| (s.cols, s.rows)),
                rect.w >= 400.0,
                theme,
            ))
            .children((harness.subagent_count() > 0).then(|| {
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .h(px(16.))
                    .px(px(5.))
                    .rounded(px(4.))
                    .bg(accent.opacity(0.14))
                    .children(
                        harness
                            .has_live_subagents()
                            .then(|| spinner(theme, accent, app.tick)),
                    )
                    .child(
                        div()
                            .font_family(theme.mono_font.clone())
                            .text_size(theme.ui(9.))
                            .text_color(accent)
                            .child(format!("{}", harness.subagent_count())),
                    )
            }))
            .children((harness.unread > 0).then(|| {
                pill(
                    format!("{}", harness.unread),
                    theme.blue,
                    theme.blue.opacity(0.18),
                    theme,
                )
            }))
            .children(exited.then(|| {
                icon_button(
                    ElementId::named_usize("restart", id.raw() as usize),
                    icons::REFRESH,
                    theme,
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.restart_harness(id);
                    cx.notify();
                }))
            }))
            .children(zoomed.then(|| {
                icon_button(
                    ElementId::named_usize("unzoom", id.raw() as usize),
                    icons::FULLSCREEN_EXIT,
                    theme,
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.zoomed = None;
                    cx.notify();
                }))
            }))
            .child(
                if close_armed {
                    div()
                        .id(ElementId::named_usize("close", id.raw() as usize))
                        .flex_shrink_0()
                        .px(px(7.))
                        .h(px(20.))
                        .flex()
                        .items_center()
                        .rounded(px(5.))
                        .bg(theme.red.opacity(0.18))
                        .border_1()
                        .border_color(theme.red.opacity(0.45))
                        .text_size(theme.ui(10.))
                        .text_color(theme.red)
                        .cursor_pointer()
                        .child("kill?")
                } else {
                    icon_button(
                        ElementId::named_usize("close", id.raw() as usize),
                        icons::CLOSE,
                        theme,
                    )
                }
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_close(id);
                    cx.notify();
                })),
            ),
    )
    .child(
        div()
            .id(ElementId::named_usize("tile-term", id.raw() as usize))
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.terminal_mouse_down(id, MouseButton::Left, event.position)
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.terminal_mouse_down(id, MouseButton::Right, event.position)
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.terminal_mouse_down(id, MouseButton::Middle, event.position)
                }),
            )
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                this.terminal_scroll(id, event);
                cx.notify();
            }))
            .relative()
            .child(match app.sessions.get(&id) {
                Some(session) => {
                    terminal::render_live(session, theme, app.tick, focused, app.cell.0)
                }
                None => terminal::render(harness, &workspace.path, theme, app.tick),
            })
            .children(exited.then(|| exited_banner(theme, accent))),
    )
    .children(drop_zone.map(|zone| drop_overlay(zone, rect, accent, theme)))
    .into_any_element()
}

fn exited_banner(theme: &Theme, accent: Hsla) -> AnyElement {
    div()
        .absolute()
        .bottom(px(10.))
        .left(px(0.))
        .right(px(0.))
        .flex()
        .justify_center()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .py(px(6.))
                .rounded(px(8.))
                .bg(theme.mantle)
                .border_1()
                .border_color(accent.opacity(0.4))
                .shadow_lg()
                .child(dot(theme.overlay, 6.))
                .child(
                    div()
                        .text_size(theme.ui(11.))
                        .text_color(theme.subtext)
                        .child("process exited"),
                )
                .child(
                    div()
                        .text_size(theme.ui(10.))
                        .text_color(theme.overlay)
                        .child("enter or ↻ restarts · ✕ removes"),
                ),
        )
        .into_any_element()
}

/// Terminal grid that fits in a tile: rect minus border, header, status bar
/// and the terminal padding.
pub fn grid_size(rect: TRect, cell: (f32, f32)) -> (u16, u16) {
    let w = rect.w - 2.0 * (BORDER + PAD_X);
    let h = rect.h - 2.0 * (BORDER + PAD_Y) - HEADER;
    (
        (w / cell.0).floor().max(2.0) as u16,
        (h / cell.1).floor().max(1.0) as u16,
    )
}

fn status_chips(
    harness: &termden_core::Harness,
    live: Option<(u16, u16)>,
    show_size: bool,
    theme: &Theme,
) -> AnyElement {
    let size = match (live, harness.kind) {
        (Some((cols, rows)), _) => format!("{cols}×{rows}"),
        (None, termden_core::HarnessKind::ClaudeCode) => "opus 4.6 · 18% context".into(),
        (None, termden_core::HarnessKind::OpenCode) => "claude-sonnet · 12% context".into(),
        (None, termden_core::HarnessKind::Pi) => "local · 8% context".into(),
        (None, termden_core::HarnessKind::Shell) => "144×44".into(),
    };

    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(8.))
        .children(show_size.then(|| {
            div()
                .px(px(6.))
                .h(px(18.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .bg(theme.surface0.opacity(0.6))
                .font_family(theme.mono_font.clone())
                .text_size(theme.ui(9.))
                .text_color(theme.overlay)
                .child(size)
        }))
        .into_any_element()
}

fn drop_overlay(active: DropZone, rect: TRect, accent: Hsla, theme: &Theme) -> AnyElement {
    let w = rect.w.max(1.0);
    let h = rect.h.max(1.0);

    let zones = [
        (
            DropZone::Left,
            6.0,
            6.0,
            (w * 0.28 - 6.0).max(0.0),
            (h - 12.0).max(0.0),
        ),
        (
            DropZone::Right,
            (w - w * 0.28).max(0.0),
            6.0,
            (w * 0.28 - 6.0).max(0.0),
            (h - 12.0).max(0.0),
        ),
        (
            DropZone::Top,
            w * 0.30,
            6.0,
            (w * 0.40).max(0.0),
            (h * 0.26 - 6.0).max(0.0),
        ),
        (
            DropZone::Bottom,
            w * 0.30,
            (h - h * 0.26).max(0.0),
            (w * 0.40).max(0.0),
            (h * 0.26 - 6.0).max(0.0),
        ),
        (
            DropZone::Center,
            w * 0.32,
            h * 0.32,
            (w * 0.36).max(0.0),
            (h * 0.36).max(0.0),
        ),
    ];

    div()
        .absolute()
        .top(px(0.))
        .left(px(0.))
        .w(px(w))
        .h(px(h))
        .children(zones.into_iter().map(|(zone, x, y, zw, zh)| {
            let is_active = zone == active;
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(zw))
                .h(px(zh))
                .rounded(px(6.))
                .border_1()
                .border_color(if is_active {
                    accent.opacity(0.85)
                } else {
                    accent.opacity(0.14)
                })
                .bg(if is_active {
                    accent.opacity(0.22)
                } else {
                    accent.opacity(0.045)
                })
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.ui(9.))
                .font_family(theme.mono_font.clone())
                .text_color(theme.text.opacity(if is_active { 0.9 } else { 0.0 }))
                .child(if is_active {
                    if zone == DropZone::Center {
                        "swap"
                    } else {
                        "split"
                    }
                } else {
                    ""
                })
        }))
        .into_any_element()
}
