use gpui::{
    AnyElement, Context, Div, ElementId, FontWeight, Hsla, MouseButton, MouseDownEvent,
    PathPromptOptions, Window, div, prelude::*, px,
};
use std::time::Instant;
use termden_core::{Harness, HarnessKind, Workspace};

use crate::state::{Mode, TermdenApp};
use crate::theme::Theme;
use crate::widgets::{
    ICON_FONT, dot, hspacer, icon_button, icons, kind_badge, pill, spinner, spinner_frame, tag_pill,
};

pub fn render(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let radius = if app.config.sidebar == termden_core::Sidebar::Pinned {
        app.config.corner_radius
    } else {
        8.0
    };

    div()
        .id("panel")
        .flex_shrink_0()
        .w(px(app.config.panel_width))
        .h_full()
        .flex()
        .flex_col()
        .rounded(px(radius))
        .bg(theme.mantle)
        .border_1()
        .border_color(theme.text.opacity(0.06))
        .overflow_hidden()
        .child(header(app, cx))
        .children(restart_banner(app, cx))
        .child(list(app, cx))
        .child(footer(app, cx))
        .into_any_element()
}

fn header(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;

    div()
        .flex()
        .items_center()
        .gap(px(9.))
        .h(px(58.))
        .px(px(14.))
        .flex_shrink_0()
        .child(
            div()
                .flex()
                .flex_1()
                .min_w(px(0.))
                // Clip the wordmark when buttons crowd it, never overlap them.
                .overflow_hidden()
                .items_center()
                .gap(px(9.))
                .on_mouse_down(MouseButton::Left, |event, window, _| {
                    // Double-click maximizes, like a title bar would.
                    if event.click_count == 2 {
                        window.zoom_window();
                    } else {
                        window.start_window_move();
                    }
                })
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(42.))
                        .rounded(px(13.))
                        .bg(theme.crust)
                        .p(px(4.5))
                        .flex()
                        .gap(px(3.))
                        .child(
                            div()
                                .flex_grow()
                                .w(px(24.))
                                .rounded(px(4.5))
                                .bg(theme.accent(0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(theme.crust)
                                .font_family(theme.mono_font.clone())
                                .text_size(theme.ui(14.))
                                .font_weight(FontWeight::BOLD)
                                .child(">"),
                        )
                        .child(
                            div()
                                .w(px(6.))
                                .flex()
                                .flex_col()
                                .gap(px(3.))
                                .child(div().flex_grow().h(px(15.5)).rounded(px(3.)).bg(theme.blue))
                                .child(div().h(px(7.)).rounded(px(3.)).bg(theme.green))
                                .child(div().h(px(4.5)).rounded(px(2.)).bg(theme.yellow)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w(px(0.))
                        .child(
                            div()
                                .text_size(theme.ui(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text)
                                .whitespace_nowrap()
                                .child("TERMDEN"),
                        )
                        .child(
                            div()
                                .flex()
                                .mt(px(-5.))
                                .items_center()
                                .gap(px(3.))
                                .child(dot(
                                    match app.mode {
                                        Mode::Live => theme.green,
                                        _ => theme.yellow,
                                    },
                                    5.,
                                ))
                                .child(
                                    div()
                                        .min_w(px(0.))
                                        .truncate()
                                        .text_size(theme.ui(10.5))
                                        .text_color(theme.overlay)
                                        .child(match app.mode {
                                            Mode::Live => {
                                                let running = app
                                                    .sessions
                                                    .values()
                                                    .filter(|s| !s.exited)
                                                    .count();
                                                match crate::pty::tmux_socket().as_str() {
                                                    "termden" => format!("{running} running"),
                                                    socket => {
                                                        format!("{running} running · {socket}")
                                                    }
                                                }
                                            }
                                            Mode::Demo => "demo fleet".into(),
                                            Mode::Mirror => "mirror".into(),
                                        }),
                                ),
                        ),
                ),
        )
        .child({
            // Window buttons join settings/close when the desktop draws no
            // title bar; then everything packs tighter to keep the wordmark.
            let (button, gap) = if app.own_chrome.is_some() {
                (22., 2.)
            } else {
                (26., 9.)
            };
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(gap))
                .children(crate::update::button(app, button, cx))
                .child(
                    icon_button("settings", icons::SETTINGS, theme)
                        .size(px(button))
                        .text_size(theme.ui(15.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings_open = true;
                            cx.notify();
                        })),
                )
                .children(app.own_chrome.map(|_| {
                    icon_button("minimize", icons::MINIMIZE, theme)
                        .size(px(button))
                        .text_size(theme.ui(14.))
                        .on_click(|_, window, _| window.minimize_window())
                }))
                .children(app.own_chrome.map(|maximized| {
                    let icon = if maximized {
                        icons::RESTORE
                    } else {
                        icons::MAXIMIZE
                    };
                    icon_button("maximize", icon, theme)
                        .size(px(button))
                        .text_size(theme.ui(13.))
                        .on_click(|_, window, _| window.zoom_window())
                }))
                .child(
                    icon_button("close", icons::CLOSE, theme)
                        .size(px(button))
                        .text_size(theme.ui(14.))
                        .on_click(cx.listener(|_, _, window, _| {
                            window.remove_window();
                        })),
                )
        })
        .into_any_element()
}

fn list(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    div()
        .id("workspace-list")
        .flex_1()
        .overflow_y_scroll()
        .px(px(9.))
        .pb(px(10.))
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(section_header(app, cx))
        .children(
            app.workspaces
                .iter()
                .filter(|w| !w.unloaded)
                .map(|workspace| workspace_card(app, workspace, cx)),
        )
        .children(unloaded_section(app, cx))
        .into_any_element()
}

fn unloaded_section(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    let theme = &app.theme;
    let parked: Vec<_> = app.workspaces.iter().filter(|w| w.unloaded).collect();
    if parked.is_empty() {
        return None;
    }
    let open = app.unloaded_open;
    let hover_bg = theme.surface0.opacity(0.32);
    Some(
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .mt(px(6.))
            .child(
                div()
                    .id("unloaded-toggle")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(4.))
                    .h(px(22.))
                    .cursor_pointer()
                    .rounded(px(6.))
                    .hover(move |style| style.bg(hover_bg))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.unloaded_open = !this.unloaded_open;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .font_family(ICON_FONT)
                            .text_size(theme.ui(14.))
                            .text_color(theme.overlay)
                            .child(if open {
                                icons::EXPAND_MORE
                            } else {
                                icons::CHEVRON_RIGHT
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.ui(9.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.overlay)
                            .child(format!("UNLOADED · {}", parked.len())),
                    ),
            )
            .children(open.then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(parked.into_iter().map(|w| {
                        let id = w.id;
                        div()
                            .id(ElementId::named_usize("unloaded", id.raw() as usize))
                            .flex()
                            .items_center()
                            .h(px(26.))
                            .px(px(10.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover_bg))
                            .text_size(theme.ui(12.))
                            .text_color(theme.subtext)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(w.name.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.activate_workspace(id);
                                cx.notify();
                            }))
                    }))
            }))
            .into_any_element(),
    )
}

fn section_header(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;

    div()
        .flex()
        .items_center()
        .h(px(28.))
        .px(px(6.))
        .mt(px(2.))
        .child(
            div()
                .text_size(theme.ui(9.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.overlay)
                .child("WORKSPACES"),
        )
        .child(hspacer())
        .child(
            icon_button("new-workspace", icons::ADD, theme).on_click(cx.listener(|_, _, _, cx| {
                let picked = cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some("Add workspace".into()),
                });
                cx.spawn(async move |this, cx| {
                    let result = picked.await;
                    let _ = this.update(cx, |this, cx| {
                        match result {
                            Ok(Ok(Some(paths))) => {
                                if let Some(dir) = paths.first() {
                                    this.add_workspace(dir);
                                }
                            }
                            Ok(Ok(None)) => {}
                            _ => this.log("Termden", "Could not open a folder picker".into(), 7),
                        }
                        cx.notify();
                    });
                })
                .detach();
            })),
        )
        .into_any_element()
}

pub fn tip(glyph: &'static str, text: &'static str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(7.))
        .child(
            div()
                .flex_shrink_0()
                .min_w(px(30.))
                .h(px(16.))
                .px(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .bg(theme.surface0.opacity(0.5))
                .border_1()
                .border_color(theme.text.opacity(0.06))
                .font_family(theme.mono_font.clone())
                .text_size(theme.ui(8.5))
                .text_color(theme.accent(0))
                .child(glyph),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.ui(9.5))
                .text_color(theme.subtext.opacity(0.8))
                .child(text),
        )
}

fn workspace_card(
    app: &TermdenApp,
    workspace: &Workspace,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let theme = &app.theme;
    let accent = theme.accent(workspace.accent);
    let active = workspace.id == app.active;
    let menu_open = app.ws_menu.is_some_and(|(w, _)| w == workspace.id);
    let hover_bg = theme.surface0.opacity(0.32);
    let id = workspace.id;
    // Collapsed and empty workspaces show no harness list, whose padding
    // otherwise spaces the card's bottom; the header pads it instead.
    let list = workspace.expanded && !workspace.harnesses.is_empty();
    let compact = !workspace.expanded && workspace.harnesses.is_empty();

    div()
        .id(ElementId::named_usize("workspace", id.raw() as usize))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                this.ws_menu = Some((
                    id,
                    (f32::from(event.position.x), f32::from(event.position.y)),
                ));
                this.adding = None;
                this.confirm_remove = None;
                cx.notify();
            }),
        )
        .flex()
        .flex_col()
        .rounded(px(9.))
        .bg(if menu_open {
            accent.opacity(0.10)
        } else if active {
            accent.opacity(0.07)
        } else {
            theme.text.opacity(0.0)
        })
        .border_1()
        .border_color(if menu_open {
            accent.opacity(0.35)
        } else if active {
            accent.opacity(0.20)
        } else {
            theme.text.opacity(0.07)
        })
        .child(
            div()
                .flex()
                .items_center()
                .h(px(if compact { 30. } else { 46. }))
                .px(px(7.))
                .mt(px(if compact { 3. } else { 6. }))
                .mb(px(if list {
                    0.
                } else if compact {
                    3.
                } else {
                    6.
                }))
                .gap(px(5.))
                .child(
                    div()
                        .id(ElementId::named_usize("workspace-main", id.raw() as usize))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .flex_1()
                        .min_w(px(0.))
                        .h_full()
                        .cursor_pointer()
                        .rounded(px(7.))
                        .hover(move |style| style.bg(hover_bg))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.activate_workspace(id);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(3.))
                                .h(px(if compact { 16. } else { 24. }))
                                .rounded(px(2.))
                                .bg(if active {
                                    accent
                                } else {
                                    theme.text.opacity(0.0)
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w(px(0.))
                                .gap(px(1.))
                                .child(
                                    div()
                                        .relative()
                                        .top(px(if compact { 0. } else { 4. }))
                                        .text_size(theme.ui(12.5))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(if active { theme.text } else { theme.subtext })
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .line_clamp(1)
                                        .child(workspace.name.clone()),
                                )
                                .children((!compact).then(|| {
                                    div()
                                        .text_size(theme.ui(10.))
                                        .text_color(theme.subtext.opacity(0.6))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .line_clamp(1)
                                        .child(if workspace.home {
                                            "general · no project".to_string()
                                        } else {
                                            workspace.path.clone()
                                        })
                                })),
                        )
                        .child(div().flex().items_center().gap(px(4.)).children(
                            (workspace.unread() > 0).then(|| {
                                pill(
                                    format!("{}", workspace.unread()),
                                    theme.blue,
                                    theme.blue.opacity(0.16),
                                    theme,
                                )
                            }),
                        )),
                )
                .child(
                    icon_button(
                        ElementId::named_usize("chevron", id.raw() as usize),
                        if workspace.expanded {
                            icons::EXPAND_MORE
                        } else {
                            icons::CHEVRON_RIGHT
                        },
                        theme,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_workspace(id);
                        cx.notify();
                    })),
                ),
        )
        .children((app.adding == Some(id)).then(|| kind_picker(app, workspace, cx)))
        .children(list.then(|| {
            div()
                .flex()
                .flex_col()
                .px(px(5.))
                .pt(px(6.))
                .pb(px(6.))
                .gap(px(1.))
                .children(workspace.harnesses.iter().map(|harness| {
                    div()
                        .flex()
                        .flex_col()
                        .child(harness_row(app, workspace, harness, cx))
                        .children(subagent_rows(harness, theme, app.tick))
                }))
        }))
        .into_any_element()
}

fn harness_row(
    app: &TermdenApp,
    workspace: &Workspace,
    harness: &Harness,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let theme = &app.theme;
    let accent = theme.agent(harness.color.unwrap_or(workspace.accent));
    let selected = app.focused == Some(harness.id);
    let hover_bg = theme.surface0.opacity(0.5);
    let id = harness.id;
    // Without a caption the row is a single line, which the flex centring then
    // puts on the row's vertical middle.
    let caption = subtitle(app, harness);

    div()
        .id(ElementId::named_usize("harness", id.raw() as usize))
        .flex()
        .items_center()
        .gap(px(7.))
        .h(px(46.))
        .pl(px(4.))
        .pr(px(7.))
        .rounded(px(7.))
        .bg(if selected {
            accent.opacity(0.13)
        } else {
            theme.text.opacity(0.0)
        })
        .hover(move |style| {
            style.bg(if selected {
                accent.opacity(0.19)
            } else {
                hover_bg
            })
        })
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.focus_harness(id);
            cx.notify();
        }))
        .child(
            div()
                .flex_shrink_0()
                .w(px(2.))
                .h(px(20.))
                .rounded(px(1.))
                .bg(if selected {
                    accent
                } else {
                    theme.text.opacity(0.0)
                }),
        )
        .child(kind_badge(harness.kind, theme))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .flex()
                                .items_center()
                                .gap(px(3.))
                                .child(if pending(harness) {
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
                                        .min_w(px(0.))
                                        .max_w_full()
                                        .flex_shrink_0()
                                        .text_size(theme.ui(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(accent)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .line_clamp(1)
                                        .child(harness.name.clone())
                                        .into_any_element()
                                })
                                .children((!harness.pronouns.is_empty()).then(|| {
                                    let mut pronouns = div();
                                    pronouns.style().flex_shrink = Some(100.);
                                    pronouns
                                        .min_w(px(0.))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .line_clamp(1)
                                        .text_size(theme.ui(9.))
                                        .text_color(theme.overlay.opacity(0.85))
                                        .child(format!("({})", harness.pronouns))
                                })),
                        )
                        .children((harness.unread > 0).then(|| {
                            pill(
                                format!("{}", harness.unread),
                                theme.blue,
                                theme.blue.opacity(0.18),
                                theme,
                            )
                        })),
                )
                .children((!caption.is_empty()).then(|| {
                    div()
                        .mt(px(-2.))
                        .text_size(theme.ui(9.5))
                        .text_color(theme.subtext.opacity(0.6))
                        .overflow_hidden()
                        .text_ellipsis()
                        .line_clamp(1)
                        .child(caption)
                })),
        )
        .children((!harness.tag.is_empty()).then(|| tag_pill(&harness.tag, theme)))
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
        .child(dot(theme.status(harness.status), 7.5))
        .into_any_element()
}

/// A harness that has not introduced itself yet: still running under the
/// auto-generated name, with no name, tag or pronouns from the agent. Shells
/// never introduce themselves, so they keep their name.
pub(crate) fn pending(harness: &Harness) -> bool {
    harness.kind != HarnessKind::Shell && !harness.announced
}

/// What a harness row says under its name: the live terminal title (tmux
/// forwards the program's), else its kind.
fn subtitle(app: &TermdenApp, harness: &Harness) -> String {
    if !harness.title.is_empty() {
        return harness.title.clone();
    }
    let title = app
        .sessions
        .get(&harness.id)
        .map(|s| s.title.trim())
        .filter(|t| !t.is_empty())
        .filter(|t| {
            let t = t
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            t != harness.kind.label().to_lowercase() && t != harness.name.to_lowercase()
        });
    match (
        title,
        app.sessions.get(&harness.id).is_some_and(|s| s.exited),
    ) {
        (_, true) => format!("{} · exited", harness.kind.label()),
        // Shell prompts title themselves `user@host:path`; the path is enough.
        (Some(title), _) => title
            .split_once(':')
            .filter(|(who, _)| who.contains('@') && !who.contains(' '))
            .map_or(title, |(_, path)| path)
            .to_string(),
        (None, _) if harness.cwd != "." => harness.cwd.clone(),
        (None, _) => String::new(),
    }
}

fn subagent_rows(harness: &Harness, theme: &Theme, tick: u64) -> Option<Div> {
    if harness.subagents.is_empty() {
        return None;
    }

    Some(
        div()
            .flex()
            .flex_col()
            .pl(px(37.))
            .pr(px(9.))
            .pb(px(5.))
            .children(
                harness
                    .subagents
                    .iter()
                    .take(3)
                    .enumerate()
                    .map(|(index, agent)| {
                        let last = index + 1 == harness.subagents.len().min(3);
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .h(px(17.))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(theme.mono_font.clone())
                                    .text_size(theme.ui(9.))
                                    .text_color(theme.text.opacity(0.22))
                                    .child(if last { "└─" } else { "├─" }),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(if agent.working {
                                        theme.mono_font.clone()
                                    } else {
                                        ICON_FONT.into()
                                    })
                                    .text_size(theme.ui(if agent.working { 9. } else { 11. }))
                                    .text_color(if agent.working {
                                        theme.accent(2)
                                    } else {
                                        theme.overlay
                                    })
                                    .child(if agent.working {
                                        spinner_frame(tick)
                                    } else {
                                        icons::CHECK
                                    }),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(theme.ui(10.))
                                    .text_color(theme.subtext)
                                    .child(agent.label.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(theme.ui(9.5))
                                    .text_color(theme.subtext.opacity(0.6))
                                    .truncate()
                                    .child(agent.detail.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(theme.mono_font.clone())
                                    .text_size(theme.ui(9.))
                                    .text_color(theme.overlay.opacity(0.85))
                                    .child(agent.elapsed.clone()),
                            )
                    }),
            ),
    )
}

fn footer(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let accent = theme.accent(0);
    let hover_bg = theme.surface0;
    let pill = |id: &'static str, label: &'static str, glyph: &'static str, open: bool| {
        let color = if open { accent } else { theme.overlay };
        div()
            .id(id)
            .flex_1()
            .h(px(26.))
            .rounded(px(6.))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .bg(accent.opacity(if open { 0.12 } else { 0.0 }))
            .hover(move |style| style.bg(hover_bg.opacity(0.5)))
            .child(
                div()
                    .font_family(ICON_FONT)
                    .text_size(theme.ui(11.))
                    .text_color(color)
                    .child(glyph),
            )
            .child(
                div()
                    .text_size(theme.ui(8.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(label),
            )
    };
    let activity_open = app.activity_on();

    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(44.))
        .px(px(9.))
        .flex_shrink_0()
        .border_t_1()
        .border_color(theme.text.opacity(0.05))
        .child(
            pill("activity-toggle", "ACTIVITY", icons::HISTORY, activity_open)
                .on_click(cx.listener(|this, _, _, cx| this.toggle_activity(cx))),
        )
        .child(div().w(px(1.)).h(px(14.)).bg(theme.text.opacity(0.12)))
        .child(
            pill("stats-toggle", "STATS", icons::STATS, app.stats_open).on_click(cx.listener(
                |this, _, _, cx| {
                    let open = !this.stats_open;
                    this.set_stats_open(open, cx);
                },
            )),
        )
        .into_any_element()
}

pub fn workspace_menu(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    let (id, (x, y)) = app.ws_menu?;
    let workspace = app.workspace(id)?;
    let theme = &app.theme;
    let hover_bg = theme.surface0.opacity(0.5);
    let row = |name: &'static str, label: &str, color: Hsla| {
        div()
            .id(ElementId::named_usize(name, id.raw() as usize))
            .px(px(10.))
            .h(px(26.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .text_size(theme.ui(12.))
            .text_color(color)
            .child(label.to_string())
    };
    let (vw, vh) = app.viewport;
    let accent = theme.accent(workspace.accent);
    let (w, h) = (170., 96.);
    let card = div()
        .id("ws-menu")
        .absolute()
        .left(px(x.min(vw - w - 6.).max(6.)))
        .top(px(y.min(vh - h - 6.).max(6.)))
        .w(px(w))
        .p(px(4.))
        .flex()
        .flex_col()
        .rounded(px(9.))
        .bg(theme.mantle)
        .border_1()
        .border_color(accent.opacity(0.35))
        .shadow_lg()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .child(
            row("menu-add", "Add agent", theme.text).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.adding = Some(id);
                    this.ws_menu = None;
                    cx.notify();
                },
            )),
        )
        .children((!workspace.home).then(|| {
            row("menu-unload", "Unload", theme.text).on_click(cx.listener(move |this, _, _, cx| {
                this.ws_menu = None;
                this.unload_workspace(id);
                cx.notify();
            }))
        }))
        .children((!workspace.home).then(|| {
            let armed = app.remove_armed(id);
            row(
                "menu-close",
                if armed {
                    "Click again to close"
                } else {
                    "Close"
                },
                theme.red,
            )
            .when(armed, |r| r.bg(theme.red.opacity(0.18)))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.remove_armed(id) {
                    this.ws_menu = None;
                    this.remove_workspace(id);
                } else {
                    this.confirm_remove = Some((id, Instant::now()));
                }
                cx.notify();
            }))
        }));
    let dismiss = |this: &mut TermdenApp,
                   _: &MouseDownEvent,
                   _: &mut Window,
                   cx: &mut Context<TermdenApp>| {
        this.ws_menu = None;
        cx.notify();
    };
    Some(
        div()
            .id("ws-menu-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(dismiss))
            .on_mouse_down(MouseButton::Right, cx.listener(dismiss))
            .child(card)
            .into_any_element(),
    )
}

fn kind_picker(
    app: &TermdenApp,
    workspace: &Workspace,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let theme = &app.theme;
    let ws = workspace.id;
    let hover_bg = theme.surface0;

    div()
        .flex()
        .gap(px(4.))
        .px(px(9.))
        .pt(px(6.))
        .pb(px(3.))
        .children(HarnessKind::ALL.into_iter().map(|kind| {
            div()
                .id(ElementId::named_usize(
                    "pick",
                    ws.raw() as usize * 8 + kind as usize,
                ))
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(3.))
                .py(px(5.))
                .rounded(px(6.))
                .border_1()
                .border_color(theme.text.opacity(0.06))
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.add_harness(ws, kind);
                    cx.notify();
                }))
                .child(kind_badge(kind, theme))
                .child(
                    div()
                        .text_size(theme.ui(8.5))
                        .text_color(theme.subtext)
                        .child(
                            kind.default_command()
                                .trim_start_matches('$')
                                .to_lowercase(),
                        ),
                )
        }))
        .into_any_element()
}

/// Activity color index; `usize::MAX` marks the human (no agent color).
pub(crate) fn entry_color(theme: &Theme, accent: usize) -> Hsla {
    if accent == usize::MAX {
        theme.text
    } else {
        theme.agent(accent)
    }
}

/// "New build ready · Restart": termden restarts only when the user clicks
/// (or presses ctrl+shift+r); × hides it until the next build.
fn restart_banner(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    let reason = app.restart_pending.as_ref()?;
    let theme = &app.theme;
    Some(
        div()
            .id("restart-banner")
            .mx(px(9.))
            .mb(px(4.))
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(theme.green.opacity(0.14))
            .border_1()
            .border_color(theme.green.opacity(0.3))
            .cursor_pointer()
            .hover(|s| s.opacity(0.85))
            .on_click(cx.listener(|this, _, _, cx| {
                this.restart_pending = None;
                this.reload();
                cx.notify();
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_size(theme.ui(10.5))
                    .text_color(theme.green)
                    .child(reason.clone()),
            )
            .child(
                div()
                    .text_size(theme.ui(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.green)
                    .child("Restart"),
            )
            .child(
                icon_button("restart-dismiss", icons::CLOSE, theme)
                    .size(px(20.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.restart_pending = None;
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .into_any_element(),
    )
}
