use gpui::{
    AnyElement, Context, Div, ElementId, FontWeight, MouseButton, PathPromptOptions, div,
    prelude::*, px,
};
use termden_core::{Harness, HarnessKind, Workspace};

use crate::state::{Mode, TermdenApp};
use crate::theme::Theme;
use crate::widgets::{
    ICON_FONT, dot, hspacer, icon_button, icons, kind_badge, pill, spinner, spinner_frame,
};

pub fn render(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let radius = app.config.corner_radius;

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
        .children(crate::choices::banner(app, cx))
        .child(list(app, cx))
        .child(activity(app))
        .child(footer(app))
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
                .items_center()
                .gap(px(9.))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
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
        .child(
            icon_button("settings", icons::SETTINGS, theme)
                .size(px(26.))
                .text_size(theme.ui(15.))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings_open = true;
                    cx.notify();
                })),
        )
        .child(
            icon_button("close", icons::CLOSE, theme)
                .size(px(26.))
                .text_size(theme.ui(14.))
                .on_click(cx.listener(|_, _, window, _| {
                    window.remove_window();
                })),
        )
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
                .map(|workspace| workspace_card(app, workspace, cx)),
        )
        .into_any_element()
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

fn activity(app: &TermdenApp) -> AnyElement {
    let theme = &app.theme;
    if app.activity_stick.replace(false) {
        app.activity_scroll.scroll_to_bottom();
    }

    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .px(px(9.))
        .pb(px(8.))
        .child(
            div().flex().items_center().h(px(24.)).px(px(6.)).child(
                div()
                    .text_size(theme.ui(9.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.overlay)
                    .child("ACTIVITY"),
            ),
        )
        .child(
            div()
                .id("activity-feed")
                .track_scroll(&app.activity_scroll)
                .overflow_y_scroll()
                .max_h(px(210.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rounded(px(9.))
                .border_1()
                .border_color(theme.text.opacity(0.05))
                .bg(theme.text.opacity(0.012))
                .p(px(8.))
                .children(app.activity.is_empty().then(|| {
                    div()
                        .text_size(theme.ui(9.))
                        .text_color(theme.overlay)
                        .child("quiet so far")
                }))
                .children(app.activity.iter().map(|entry| {
                    let color = theme.accent(entry.accent);
                    div()
                        .flex()
                        .items_start()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex_shrink_0()
                                .mt(px(4.))
                                .size(px(5.))
                                .rounded_full()
                                .bg(color),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_wrap()
                                .gap_x(px(5.))
                                .text_size(theme.ui(10.))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(color)
                                        .child(entry.name.clone()),
                                )
                                .child(
                                    div().text_color(theme.subtext).child(entry.message.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .mt(px(1.))
                                .font_family(theme.mono_font.clone())
                                .text_size(theme.ui(8.5))
                                .text_color(theme.overlay.opacity(0.8))
                                .child(entry.time.clone()),
                        )
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
    let hover_bg = theme.surface0.opacity(0.32);
    let id = workspace.id;

    div()
        .id(ElementId::named_usize("workspace", id.raw() as usize))
        .flex()
        .flex_col()
        .rounded(px(9.))
        .bg(if active {
            accent.opacity(0.07)
        } else {
            theme.text.opacity(0.0)
        })
        .border_1()
        .border_color(if active {
            accent.opacity(0.20)
        } else {
            theme.text.opacity(0.0)
        })
        .child(
            div()
                .flex()
                .items_center()
                .h(px(46.))
                .px(px(7.))
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
                                .h(px(24.))
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
                                        .text_size(theme.ui(12.5))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(if active { theme.text } else { theme.subtext })
                                        .truncate()
                                        .child(workspace.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(theme.ui(10.))
                                        .text_color(theme.subtext.opacity(0.6))
                                        .truncate()
                                        .child(workspace.path.clone()),
                                ),
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
                        ElementId::named_usize("ws-add", id.raw() as usize),
                        icons::ADD,
                        theme,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.adding = if this.adding == Some(id) {
                            None
                        } else {
                            Some(id)
                        };
                        this.confirm_remove = None;
                        cx.notify();
                    })),
                )
                .children(active.then(|| {
                    icon_button(
                        ElementId::named_usize("ws-remove", id.raw() as usize),
                        icons::CLOSE,
                        theme,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm_remove = Some(id);
                        this.adding = None;
                        cx.notify();
                    }))
                }))
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
        .children((app.confirm_remove == Some(id)).then(|| confirm_remove(app, workspace, cx)))
        .children(workspace.expanded.then(|| {
            div()
                .flex()
                .flex_col()
                .px(px(5.))
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
    let accent = theme.accent(workspace.accent);
    let selected = app.focused == Some(harness.id);
    let hover_bg = theme.surface0.opacity(0.5);
    let id = harness.id;

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
                                .flex()
                                .items_baseline()
                                .gap(px(5.))
                                .child(
                                    div()
                                        .min_w(px(0.))
                                        .text_size(theme.ui(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(if selected {
                                            theme.text
                                        } else {
                                            theme.subtext
                                        })
                                        .truncate()
                                        .child(harness.name.clone()),
                                )
                                .children((!harness.pronouns.is_empty()).then(|| {
                                    div()
                                        .flex_shrink_0()
                                        .text_size(theme.ui(9.))
                                        .text_color(theme.overlay)
                                        .child(harness.pronouns.clone())
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
                .child(
                    div()
                        .mt(px(-2.))
                        .text_size(theme.ui(9.5))
                        .text_color(theme.subtext.opacity(0.6))
                        .truncate()
                        .child(subtitle(app, harness)),
                ),
        )
        .children((!harness.tag.is_empty()).then(|| {
            let color = theme.accent(
                harness
                    .tag
                    .to_lowercase()
                    .bytes()
                    .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize)),
            );
            div()
                .flex_shrink_0()
                .h(px(18.))
                .px(px(6.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .bg(color.opacity(0.18))
                .text_color(color)
                .font_family(theme.mono_font.clone())
                .text_size(theme.ui(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(harness.tag.to_uppercase())
        }))
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

fn footer(app: &TermdenApp) -> AnyElement {
    let theme = &app.theme;
    let working = app.working_count();
    let total = app.harness_count();

    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(44.))
        .px(px(13.))
        .flex_shrink_0()
        .border_t_1()
        .border_color(theme.text.opacity(0.05))
        .child(dot(
            if working > 0 {
                theme.green
            } else {
                theme.overlay
            },
            5.,
        ))
        .child(
            div()
                .text_size(theme.ui(9.5))
                .text_color(theme.overlay)
                .child(format!(
                    "{working} working · {total} harness{}",
                    if total == 1 { "" } else { "es" }
                )),
        )
        .into_any_element()
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
        .pb(px(6.))
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

fn confirm_remove(
    app: &TermdenApp,
    workspace: &Workspace,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let theme = &app.theme;
    let ws = workspace.id;
    let count = workspace.harnesses.len();

    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .mx(px(9.))
        .mb(px(6.))
        .p(px(7.))
        .rounded(px(6.))
        .bg(theme.red.opacity(0.08))
        .border_1()
        .border_color(theme.red.opacity(0.3))
        .child(
            div()
                .flex_1()
                .text_size(theme.ui(9.5))
                .text_color(theme.subtext)
                .child(format!(
                    "remove and kill {count} harness{}?",
                    if count == 1 { "" } else { "es" }
                )),
        )
        .child(
            div()
                .id(ElementId::named_usize("confirm-remove", ws.raw() as usize))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(4.))
                .bg(theme.red.opacity(0.2))
                .text_size(theme.ui(9.5))
                .text_color(theme.red)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.remove_workspace(ws);
                    cx.notify();
                }))
                .child("remove"),
        )
        .child(
            div()
                .id(ElementId::named_usize("cancel-remove", ws.raw() as usize))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(4.))
                .text_size(theme.ui(9.5))
                .text_color(theme.overlay)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm_remove = None;
                    cx.notify();
                }))
                .child("cancel"),
        )
        .into_any_element()
}
