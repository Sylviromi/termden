use std::collections::HashMap;

use termden_core::{
    AppState, Axis, Harness, HarnessId, HarnessKind, HarnessStatus, LayoutNode, Subagent,
    SubagentId, Workspace, WorkspaceId,
};

use crate::state::{Activity, TermdenApp};

/// A believable little fleet so the first launch shows every visual state:
/// working, idle, needs-input, error, unread mail, live subagents.
pub fn build() -> AppState {
    let wsp = WorkspaceId::new(1);
    let api = WorkspaceId::new(2);
    let infra = WorkspaceId::new(3);

    let workspaces = vec![
        Workspace {
            id: wsp,
            name: "termden".into(),
            path: "~/code/termden".into(),
            accent: 0,
            expanded: true,
            harnesses: vec![
                Harness {
                    id: HarnessId::new(1),
                    workspace: wsp,
                    name: "claude".into(),
                    kind: HarnessKind::ClaudeCode,
                    cwd: "crates/app".into(),
                    status: HarnessStatus::Working,
                    subagents: vec![
                        sub(1, "explore", "scanning crates/", "1:12", true),
                        sub(2, "tests", "writing hit cases", "0:48", true),
                    ],
                    unread: 0,
                    tag: "backend".into(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
                Harness {
                    id: HarnessId::new(2),
                    workspace: wsp,
                    name: "opencode".into(),
                    kind: HarnessKind::OpenCode,
                    cwd: ".".into(),
                    status: HarnessStatus::NeedsInput,
                    subagents: vec![sub(3, "review", "checking canvas.rs", "0:31", true)],
                    unread: 3,
                    tag: "reviewer".into(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
                Harness {
                    id: HarnessId::new(3),
                    workspace: wsp,
                    name: "shell".into(),
                    kind: HarnessKind::Shell,
                    cwd: ".".into(),
                    status: HarnessStatus::Idle,
                    subagents: vec![],
                    unread: 0,
                    tag: String::new(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
            ],
        },
        Workspace {
            id: api,
            name: "api".into(),
            path: "~/src/api".into(),
            accent: 1,
            expanded: true,
            harnesses: vec![
                Harness {
                    id: HarnessId::new(4),
                    workspace: api,
                    name: "claude".into(),
                    kind: HarnessKind::ClaudeCode,
                    cwd: ".".into(),
                    status: HarnessStatus::Idle,
                    subagents: vec![],
                    unread: 0,
                    tag: String::new(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
                Harness {
                    id: HarnessId::new(5),
                    workspace: api,
                    name: "opencode".into(),
                    kind: HarnessKind::OpenCode,
                    cwd: "services/auth".into(),
                    status: HarnessStatus::Working,
                    subagents: vec![sub(4, "build", "cargo check", "2:04", true)],
                    unread: 1,
                    tag: String::new(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
            ],
        },
        Workspace {
            id: infra,
            name: "infra".into(),
            path: "~/infra".into(),
            accent: 3,
            expanded: false,
            harnesses: vec![
                Harness {
                    id: HarnessId::new(6),
                    workspace: infra,
                    name: "opencode".into(),
                    kind: HarnessKind::OpenCode,
                    cwd: ".".into(),
                    status: HarnessStatus::Error,
                    subagents: vec![],
                    unread: 0,
                    tag: String::new(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
                Harness {
                    id: HarnessId::new(7),
                    workspace: infra,
                    name: "claude".into(),
                    kind: HarnessKind::ClaudeCode,
                    cwd: "terraform".into(),
                    status: HarnessStatus::Working,
                    subagents: vec![
                        sub(5, "audit", "tf plan", "3:11", true),
                        sub(6, "plan", "waiting for input", "0:09", false),
                    ],
                    unread: 2,
                    tag: String::new(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
                Harness {
                    id: HarnessId::new(8),
                    workspace: infra,
                    name: "shell".into(),
                    kind: HarnessKind::Shell,
                    cwd: ".".into(),
                    status: HarnessStatus::Idle,
                    subagents: vec![],
                    unread: 0,
                    tag: String::new(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                },
            ],
        },
    ];

    let mut layouts = HashMap::new();
    layouts.insert(
        wsp,
        LayoutNode::split(
            Axis::Horizontal,
            0.61,
            LayoutNode::leaf(HarnessId::new(1)),
            LayoutNode::split(
                Axis::Vertical,
                0.52,
                LayoutNode::leaf(HarnessId::new(2)),
                LayoutNode::leaf(HarnessId::new(3)),
            ),
        ),
    );
    layouts.insert(
        api,
        LayoutNode::split(
            Axis::Horizontal,
            0.56,
            LayoutNode::leaf(HarnessId::new(4)),
            LayoutNode::leaf(HarnessId::new(5)),
        ),
    );
    layouts.insert(
        infra,
        LayoutNode::split(
            Axis::Horizontal,
            0.46,
            LayoutNode::leaf(HarnessId::new(6)),
            LayoutNode::split(
                Axis::Vertical,
                0.5,
                LayoutNode::leaf(HarnessId::new(7)),
                LayoutNode::leaf(HarnessId::new(8)),
            ),
        ),
    );

    AppState {
        workspaces,
        layouts,
        active: Some(wsp),
        focused: Some(HarnessId::new(1)),
        ..Default::default()
    }
}

pub fn seed_activity(app: &mut TermdenApp) {
    let feed = [
        (191, "infra", "tf plan needs review", 3),
        (124, "api", "build check passed", 1),
        (91, "opencode", "waiting for input", 0),
        (48, "claude", "tests finished", 0),
        (12, "claude", "spawned explore + tests", 0),
    ];
    for (ago, name, message, accent) in feed {
        app.activity.push(Activity {
            time: crate::state::clock(ago),
            name: name.into(),
            message: message.into(),
            accent,
            to: None,
            harness: None,
        });
    }
}

fn sub(id: u32, label: &str, detail: &str, elapsed: &str, working: bool) -> Subagent {
    Subagent {
        id: SubagentId::new(id),
        label: label.into(),
        detail: detail.into(),
        elapsed: elapsed.into(),
        working,
    }
}

impl TermdenApp {
    /// Total harnesses across every workspace, for the footer tally.
    pub fn harness_count(&self) -> usize {
        self.workspaces
            .iter()
            .map(|workspace| workspace.harnesses.len())
            .sum()
    }

    pub fn working_count(&self) -> usize {
        self.workspaces
            .iter()
            .map(|workspace| workspace.working())
            .sum()
    }
}
