//! Per-workspace usage stats: tokens per agent, MCP servers, uptime.
//!
//! The numbers come from the agents themselves (Claude Code transcripts,
//! opencode's database) and from tmux's session start time. Collection runs on
//! the background executor every few seconds so a large transcript never
//! stalls a frame.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::{AnyElement, Context, Div, FontWeight, Hsla, Timer, div, prelude::*, px, relative};
use termden_core::stats::{self, ClaudeLog, ClaudeStatus, Limit, Usage, short_age, short_tokens};
use termden_core::{Harness, HarnessId, HarnessKind, Workspace, WorkspaceId};

use crate::state::{Mode, TermdenApp};
use crate::theme::Theme;
use crate::widgets::{dot, hspacer, icon_button, icons, kind_badge};

const REFRESH: Duration = Duration::from_secs(5);

/// What the UI reads. Empty until the first collection finishes.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StatsCache {
    pub workspaces: HashMap<WorkspaceId, WorkspaceStats>,
    /// Plan limits per provider account (not per tile or workspace).
    pub plans: Vec<Plan>,
    pub at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub name: &'static str,
    pub limits: Vec<Limit>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct WorkspaceStats {
    pub agents: Vec<AgentStats>,
    /// Every MCP server enabled for at least one agent in the workspace.
    pub mcp: Vec<String>,
    /// Start of the oldest live harness, as unix seconds.
    pub started: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentStats {
    pub id: HarnessId,
    /// `None` when the agent reports no usage (pi) or its session data is
    /// not readable yet.
    pub usage: Option<Usage>,
    /// How full the context window is, from Claude's status line.
    pub context_percent: Option<f64>,
    pub mcp: Vec<String>,
    pub started: Option<u64>,
    /// Whether termden knows the harness's provider sessions. Claude tiles
    /// started without the SessionStart hook have none (a new tile tracks).
    pub tracked: bool,
}

impl WorkspaceStats {
    /// All token counters summed over the workspace's agents; the pane shows
    /// input + output, cached reads separately.
    pub fn usage(&self) -> Usage {
        let mut sum = Usage::default();
        for usage in self.agents.iter().filter_map(|agent| agent.usage) {
            sum.add(usage);
        }
        sum
    }

    pub fn agent(&self, id: HarnessId) -> Option<&AgentStats> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    pub fn age(&self) -> Option<u64> {
        self.started.map(age_now)
    }

    /// Whether any agent reported usage. A workspace whose tiles have not
    /// spoken yet has no totals, which is not the same as zero tokens.
    pub fn known(&self) -> bool {
        self.agents.iter().any(|agent| agent.usage.is_some())
    }
}

impl AgentStats {
    pub fn age(&self) -> Option<u64> {
        self.started.map(age_now)
    }
}

fn age_now(started: u64) -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|now| now.as_secs().saturating_sub(started))
        .unwrap_or(0)
}

/// Start collecting; one loop for the life of the app.
pub fn watch(cx: &mut Context<TermdenApp>) {
    cx.spawn(async move |this, cx| {
        // Claude transcripts are read incrementally; keep the readers here.
        let logs: Arc<Mutex<HashMap<String, ClaudeLog>>> = Arc::default();
        // opencode Go limits cost a process start; ask once a minute.
        let go: Arc<Mutex<GoCache>> = Arc::default();
        loop {
            let Ok(request) = this.update(cx, |this, _| this.stats_request()) else {
                break;
            };
            let (logs, go) = (logs.clone(), go.clone());
            let collected = cx
                .background_executor()
                .spawn(async move { collect(request, &logs, &go) })
                .await;
            let alive = this.update(cx, |this, cx| {
                this.stats.at = Some(Instant::now());
                let (workspaces, plans) = collected;
                if this.stats.workspaces != workspaces || this.stats.plans != plans {
                    this.stats.workspaces = workspaces;
                    this.stats.plans = plans;
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
            Timer::after(REFRESH).await;
        }
    })
    .detach();
}

/// A cheap copy of what collection needs, taken on the UI thread.
pub struct Request {
    demo: bool,
    workspaces: Vec<RequestWorkspace>,
}

struct RequestWorkspace {
    id: WorkspaceId,
    agents: Vec<RequestAgent>,
}

struct RequestAgent {
    id: HarnessId,
    kind: HarnessKind,
    cwd: PathBuf,
}

impl TermdenApp {
    fn stats_request(&self) -> Request {
        Request {
            demo: self.mode == Mode::Demo,
            workspaces: self
                .workspaces
                .iter()
                .map(|workspace| RequestWorkspace {
                    id: workspace.id,
                    // Shells are the user's terminals; they use no tokens.
                    agents: workspace
                        .harnesses
                        .iter()
                        .filter(|harness| harness.kind != HarnessKind::Shell)
                        .map(|harness| RequestAgent {
                            id: harness.id,
                            kind: harness.kind,
                            cwd: crate::state::resolve_cwd(&workspace.path, &harness.cwd),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

type Collected = (HashMap<WorkspaceId, WorkspaceStats>, Vec<Plan>);

/// When opencode Go limits were last asked for, and the answer.
type GoCache = (Option<Instant>, Option<Vec<Limit>>);

const GO_REFRESH: Duration = Duration::from_secs(60);

fn collect(
    request: Request,
    logs: &Mutex<HashMap<String, ClaudeLog>>,
    go: &Mutex<GoCache>,
) -> Collected {
    if request.demo {
        return demo(&request);
    }
    // Claude limits come from the newest status line of any Claude tile.
    let mut claude: Option<ClaudeStatus> = None;
    // One tmux call for every harness: its session start time.
    let starts = crate::pty::session_starts();
    let mut out = HashMap::new();
    let mut active: Vec<String> = Vec::new();
    for workspace in &request.workspaces {
        let mut agents = Vec::new();
        let mut mcp: Vec<String> = Vec::new();
        let mut started: Option<u64> = None;
        for agent in &workspace.agents {
            let start = starts.get(&agent.id.raw()).copied();
            let since_ms = start.map_or(0, |start| start * 1000);
            // Every provider session the tile went through (`/clear`, `/new`),
            // so clearing a conversation does not erase what it cost.
            let sessions = match agent.kind {
                HarnessKind::ClaudeCode => stats::claude_sessions(agent.id),
                HarnessKind::OpenCode => stats::opencode_sessions(agent.id, since_ms),
                _ => Vec::new(),
            };
            let tracked = match agent.kind {
                HarnessKind::ClaudeCode => !sessions.is_empty(),
                HarnessKind::OpenCode => true,
                _ => false,
            };
            let usage = sessions
                .iter()
                .filter_map(|session| match agent.kind {
                    HarnessKind::ClaudeCode => {
                        active.push(session.clone());
                        let mut guard = logs.lock().unwrap_or_else(|error| error.into_inner());
                        let log = guard
                            .entry(session.clone())
                            .or_insert_with(|| ClaudeLog::new(session));
                        log.refresh();
                        log.found().then(|| log.usage())
                    }
                    _ => stats::opencode_usage(session),
                })
                .reduce(|mut sum, usage| {
                    sum.add(usage);
                    sum
                });
            let status = (agent.kind == HarnessKind::ClaudeCode)
                .then(|| stats::claude_status(agent.id))
                .flatten();
            if let Some(status) = status.as_ref().filter(|status| !status.limits.is_empty())
                && claude.as_ref().is_none_or(|newest| newest.at < status.at)
            {
                claude = Some(status.clone());
            }
            let agent_mcp = stats::mcp_servers(&agent.cwd, agent.id, agent.kind);
            for name in &agent_mcp {
                if !mcp.contains(name) {
                    mcp.push(name.clone());
                }
            }
            started = match (started, start) {
                (Some(oldest), Some(start)) => Some(oldest.min(start)),
                (oldest, None) => oldest,
                (None, start) => start,
            };
            agents.push(AgentStats {
                id: agent.id,
                usage,
                context_percent: status.and_then(|status| status.context_percent),
                mcp: agent_mcp,
                started: start,
                tracked,
            });
        }
        mcp.sort();
        out.insert(
            workspace.id,
            WorkspaceStats {
                agents,
                mcp,
                started,
            },
        );
    }
    // A transcript reader holds every message id it saw; drop the ones whose
    // harness is gone so a long-lived app does not keep growing.
    if let Ok(mut logs) = logs.lock() {
        logs.retain(|session, _| active.contains(session));
    }
    let mut plans: Vec<Plan> = claude
        .map(|status| Plan {
            name: "Claude",
            limits: status.limits,
        })
        .into_iter()
        .collect();
    let opencode = request
        .workspaces
        .iter()
        .flat_map(|workspace| &workspace.agents)
        .any(|agent| agent.kind == HarnessKind::OpenCode);
    if opencode {
        let mut go = go.lock().unwrap_or_else(|error| error.into_inner());
        if go.0.is_none_or(|at| at.elapsed() >= GO_REFRESH) {
            *go = (Some(Instant::now()), stats::opencode_go_limits());
        }
        if let Some(limits) = go.1.clone() {
            plans.push(Plan {
                name: "opencode Go",
                limits,
            });
        }
    }
    (out, plans)
}

/// The demo fleet has no real sessions or transcripts; these keep every state
/// visible (an agent with no usage, an errored agent, plan limits).
fn demo(request: &Request) -> Collected {
    let usage = |id: u32| match id {
        1 => Some(Usage {
            input: 98_200,
            output: 12_400,
            cache_read: 620_000,
            cache_write: 24_000,
        }),
        2 => Some(Usage {
            input: 103_000,
            output: 8_900,
            cache_read: 380_000,
            cache_write: 0,
        }),
        4 => Some(Usage {
            input: 42_100,
            output: 6_200,
            cache_read: 210_000,
            cache_write: 8_000,
        }),
        5 => Some(Usage {
            input: 39_400,
            output: 4_100,
            cache_read: 150_000,
            cache_write: 0,
        }),
        6 => Some(Usage {
            input: 9_100,
            output: 700,
            cache_read: 30_000,
            cache_write: 0,
        }),
        7 => Some(Usage {
            input: 61_800,
            output: 7_300,
            cache_read: 340_000,
            cache_write: 12_000,
        }),
        _ => None,
    };
    let started = |id: u32| -> u64 {
        match id {
            1 => 8_040,
            2 => 2_460,
            3 => 10_920,
            4 => 1_080,
            5 => 3_960,
            6 => 300,
            7 => 12_720,
            8 => 6_600,
            _ => 3_600,
        }
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|now| now.as_secs())
        .unwrap_or(0);

    let mut out = HashMap::new();
    for workspace in &request.workspaces {
        let mut agents = Vec::new();
        let mut mcp: Vec<String> = Vec::new();
        let mut oldest: Option<u64> = None;
        for agent in &workspace.agents {
            let names: Vec<String> = match agent.kind {
                HarnessKind::ClaudeCode => ["github", "playwright", "termden"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                HarnessKind::OpenCode => ["fs", "termden"].into_iter().map(String::from).collect(),
                _ => Vec::new(),
            };
            for name in &names {
                if !mcp.contains(name) {
                    mcp.push(name.clone());
                }
            }
            let start = Some(now.saturating_sub(started(agent.id.raw())));
            oldest = match (oldest, start) {
                (Some(old), Some(start)) => Some(old.min(start)),
                (old, None) => old,
                (None, start) => start,
            };
            agents.push(AgentStats {
                id: agent.id,
                usage: usage(agent.id.raw()),
                context_percent: (agent.kind == HarnessKind::ClaudeCode)
                    .then(|| [38., 12., 71.][agent.id.raw() as usize % 3]),
                mcp: names,
                started: start,
                tracked: matches!(agent.kind, HarnessKind::ClaudeCode | HarnessKind::OpenCode),
            });
        }
        mcp.sort();
        out.insert(
            workspace.id,
            WorkspaceStats {
                agents,
                mcp,
                started: oldest,
            },
        );
    }
    let limit = |label, used_percent, resets_in| Limit {
        label,
        used_percent,
        resets_at: Some(now + resets_in),
    };
    let plans = vec![
        Plan {
            name: "Claude",
            limits: vec![
                limit("5h session", 42., 2 * 3600 + 180),
                limit("week", 18., 4 * 86_400 + 5 * 3600),
            ],
        },
        Plan {
            name: "opencode Go",
            limits: vec![
                limit("5h session", 86., 3_000),
                limit("week", 8., 5 * 86_400),
                limit("month", 4., 17 * 86_400),
            ],
        },
    ];
    (out, plans)
}

// ---------------------------------------------------------------- the pane

/// Width of the stats pane, not counting the gap around it: narrower in small
/// windows so the tiles keep room to work.
pub fn width(viewport_width: f32) -> f32 {
    (viewport_width * 0.25).clamp(260., 360.)
}

/// Plan limits on top (per account), then the active workspace: its total,
/// where the tokens went, and one row per agent. Shells use no tokens and are
/// not listed.
pub fn render(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let workspace = app.workspace(app.active);
    let stats = workspace.and_then(|workspace| app.stats.workspaces.get(&workspace.id));
    let agents: Vec<&Harness> = workspace
        .map(|workspace| {
            workspace
                .harnesses
                .iter()
                .filter(|harness| harness.kind != HarnessKind::Shell)
                .collect()
        })
        .unwrap_or_default();

    div()
        .id("stats-pane")
        .flex_shrink_0()
        .w(px(width(app.viewport.0)))
        .h_full()
        .flex()
        .flex_col()
        .rounded(px(app.config.corner_radius))
        .bg(theme.mantle)
        .border_1()
        .border_color(theme.text.opacity(0.06))
        .overflow_hidden()
        .child(
            div()
                .flex_shrink_0()
                .h(px(40.))
                .pl(px(16.))
                .pr(px(10.))
                .flex()
                .items_center()
                .border_b_1()
                .border_color(theme.text.opacity(0.06))
                .child(label("USAGE", theme))
                .child(hspacer())
                .child(
                    icon_button("stats-close", icons::CLOSE, theme)
                        .on_click(cx.listener(|this, _, _, cx| this.set_stats_open(false, cx))),
                ),
        )
        // One scroll for everything but the header and MCP: short windows
        // must still reach the agents.
        .child(
            div()
                .id("stats-list")
                .track_scroll(&app.stats_scroll)
                .flex_1()
                .overflow_y_scroll()
                .child(limits(app))
                .children(workspace.map(|workspace| summary(workspace, stats, agents.len(), theme)))
                .child(
                    div()
                        .px(px(16.))
                        .pt(px(14.))
                        .child(label("AGENTS", theme))
                        .children(workspace.into_iter().flat_map(|workspace| {
                            agents.iter().enumerate().map(move |(index, harness)| {
                                agent_row(workspace, harness, stats, index, theme)
                            })
                        }))
                        .children(agents.is_empty().then(|| {
                            div()
                                .py(px(10.))
                                .text_size(theme.ui(9.5))
                                .text_color(theme.overlay)
                                .child("No agents in this workspace.")
                        })),
                ),
        )
        .child(mcp(stats.map_or(&[][..], |stats| &stats.mcp), theme))
        .into_any_element()
}

fn label(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.ui(8.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.overlay)
        .child(text)
}

fn section(theme: &Theme) -> Div {
    div()
        .flex_shrink_0()
        .px(px(16.))
        .py(px(14.))
        .border_b_1()
        .border_color(theme.text.opacity(0.06))
        .flex()
        .flex_col()
}

/// A rounded track with coloured parts; `parts` are fractions of the track.
fn bar(parts: &[(f32, Hsla)], height: f32, theme: &Theme) -> Div {
    div()
        .w_full()
        .h(px(height))
        .rounded(px(height / 2.))
        .bg(theme.text.opacity(0.08))
        .overflow_hidden()
        .flex()
        .children(
            parts
                .iter()
                .map(|&(part, color)| div().h_full().w(relative(part.clamp(0., 1.))).bg(color)),
        )
}

/// One block per provider account in use, each window a meter.
fn limits(app: &TermdenApp) -> AnyElement {
    let theme = &app.theme;
    let plans = &app.stats.plans;
    let now = now();

    section(theme)
        .gap(px(10.))
        .child(label("PLAN LIMITS", theme))
        .children(plans.iter().map(|plan| {
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(theme.ui(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.subtext)
                        .child(plan.name),
                )
                .children(plan.limits.iter().map(|limit| meter(limit, now, theme)))
        }))
        .children(plans.is_empty().then(|| {
            div()
                .text_size(theme.ui(9.))
                .text_color(theme.overlay)
                .child(
                    "Claude reports limits only to tiles started by this version: open a \
                     new Claude tile. opencode Go shows while an opencode tile is open.",
                )
        }))
        .into_any_element()
}

fn meter(limit: &Limit, now: u64, theme: &Theme) -> Div {
    // A window that has rolled over since the last report starts empty.
    let past = limit.resets_at.is_some_and(|at| at <= now);
    let used = if past { 0. } else { limit.used_percent };
    let color = match used {
        u if u >= 85. => theme.red,
        u if u >= 60. => theme.yellow,
        _ => theme.green,
    };
    let resets = match limit.resets_at {
        Some(_) if past => "reset".to_string(),
        Some(at) => reset_text(at, now),
        None => String::new(),
    };

    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(theme.ui(10.))
                        .text_color(theme.text)
                        .child(limit.label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .font_family(theme.mono_font.clone())
                        .text_size(theme.ui(8.5))
                        .text_color(theme.overlay)
                        .child(resets),
                )
                .child(
                    div()
                        .font_family(theme.mono_font.clone())
                        .text_size(theme.ui(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color)
                        .child(format!("{used:.0}%")),
                ),
        )
        .child(bar(&[(used as f32 / 100., color)], 6., theme))
}

/// `resets in 2h 03m` within a day, `resets Mon 09:00` later.
fn reset_text(at: u64, now: u64) -> String {
    if at - now < 86_400 {
        format!("resets in {}", short_age(at - now))
    } else {
        let tm = local(at);
        let time = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        format!("resets {} {time}", DAYS[tm.tm_wday.rem_euclid(7) as usize])
    }
}

fn local(at: u64) -> libc::tm {
    // SAFETY: localtime_r only writes into the zeroed tm we pass.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&(at as libc::time_t), &mut tm);
        tm
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|now| now.as_secs())
        .unwrap_or(0)
}

/// The workspace's name, its total and where the tokens went.
fn summary(
    workspace: &Workspace,
    stats: Option<&WorkspaceStats>,
    agents: usize,
    theme: &Theme,
) -> Div {
    let known = stats.is_some_and(WorkspaceStats::known);
    let usage = stats.map(WorkspaceStats::usage).unwrap_or_default();
    let mut meta = match agents {
        1 => "1 agent".to_string(),
        n => format!("{n} agents"),
    };
    if let Some(age) = stats.and_then(WorkspaceStats::age) {
        meta.push_str(&format!(" · up {}", short_age(age)));
    }
    let total = usage.total().max(1) as f32;
    let parts = [
        (usage.cache_read, theme.mauve, "cached"),
        (usage.fresh_input(), theme.blue, "in"),
        (usage.output, theme.green, "out"),
    ];

    section(theme)
        .gap(px(10.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(dot(theme.accent(workspace.accent), 6.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .text_size(theme.ui(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(workspace.name.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(theme.ui(9.))
                        .text_color(theme.overlay)
                        .child(meta),
                ),
        )
        .child(
            div()
                .flex()
                // GPUI 0.2.2 does not honour baseline alignment: sit the label
                // on the number's baseline by hand.
                .items_end()
                .gap(px(8.))
                .child(
                    div()
                        .font_family(theme.mono_font.clone())
                        .text_size(theme.ui(24.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if known { theme.text } else { theme.overlay })
                        .child(if known {
                            short_tokens(usage.total())
                        } else {
                            "—".into()
                        }),
                )
                .child(
                    div()
                        .pb(theme.ui(4.))
                        .text_size(theme.ui(9.5))
                        .text_color(theme.overlay)
                        .child("tokens"),
                ),
        )
        .children(known.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(bar(
                    &parts.map(|(value, color, _)| (value as f32 / total, color.opacity(0.8))),
                    7.,
                    theme,
                ))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(12.))
                        .font_family(theme.mono_font.clone())
                        .text_size(theme.ui(8.5))
                        .children(parts.map(|(value, color, name)| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .child(div().size(px(6.)).rounded(px(1.5)).bg(color))
                                .child(div().text_color(theme.subtext).child(short_tokens(value)))
                                .child(div().text_color(theme.overlay).child(name))
                        })),
                )
        }))
}

fn agent_row(
    workspace: &Workspace,
    harness: &Harness,
    stats: Option<&WorkspaceStats>,
    index: usize,
    theme: &Theme,
) -> Div {
    let agent = stats.and_then(|stats| stats.agent(harness.id));
    let usage = agent.and_then(|agent| agent.usage);
    let accent = theme.agent(harness.color.unwrap_or(workspace.accent));
    let mut detail = match usage {
        Some(usage) => format!(
            "in {} · out {}",
            short_tokens(usage.fresh_input()),
            short_tokens(usage.output)
        ),
        None if harness.kind == HarnessKind::Pi => "pi reports no usage".to_string(),
        None if agent.is_some_and(|agent| !agent.tracked) => {
            "restart the tile to track usage".to_string()
        }
        None => "no reply yet".to_string(),
    };
    if let Some(age) = agent.and_then(AgentStats::age) {
        detail.push_str(&format!(" · up {}", short_age(age)));
    }
    let context = agent.and_then(|agent| agent.context_percent);
    let mono = theme.mono_font.clone();

    div()
        .py(px(11.))
        .when(index > 0, |row| {
            row.border_t_1().border_color(theme.text.opacity(0.05))
        })
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .child(kind_badge(harness.kind, theme))
                .child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(140.))
                        .truncate()
                        .text_size(theme.ui(10.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(accent)
                        .child(harness.name.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .text_size(theme.ui(9.))
                        .text_color(theme.overlay)
                        .child(harness.tag.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(mono.clone())
                        .text_size(theme.ui(10.))
                        .text_color(theme.subtext)
                        .child(usage.map_or(String::new(), |usage| short_tokens(usage.total()))),
                ),
        )
        .children(context.map(|percent| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .font_family(mono.clone())
                .text_size(theme.ui(8.5))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(theme.overlay)
                        .child("context"),
                )
                .child(div().flex_1().child(bar(
                    &[(percent as f32 / 100., theme.peach)],
                    5.,
                    theme,
                )))
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(30.))
                        .flex()
                        .justify_end()
                        .text_color(theme.subtext)
                        .child(format!("{percent:.0}%")),
                )
        }))
        .child(
            div()
                .truncate()
                .font_family(mono)
                .text_size(theme.ui(8.5))
                .text_color(theme.overlay)
                .child(detail),
        )
}

fn mcp(servers: &[String], theme: &Theme) -> Div {
    div()
        .flex_shrink_0()
        .px(px(16.))
        .py(px(12.))
        .border_t_1()
        .border_color(theme.text.opacity(0.06))
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(label("MCP SERVERS", theme))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(5.))
                .children(servers.iter().map(|name| {
                    div()
                        .h(px(18.))
                        .px(px(7.))
                        .rounded(px(4.))
                        .bg(theme.text.opacity(0.07))
                        .flex()
                        .items_center()
                        .font_family(theme.mono_font.clone())
                        .text_size(theme.ui(8.5))
                        .text_color(theme.subtext)
                        .child(name.clone())
                }))
                .children(servers.is_empty().then(|| {
                    div()
                        .text_size(theme.ui(8.5))
                        .text_color(theme.overlay)
                        .child("none")
                })),
        )
}
