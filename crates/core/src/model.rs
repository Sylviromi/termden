use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl $name {
            pub const fn new(value: u32) -> Self {
                Self(value)
            }

            pub const fn raw(self) -> u32 {
                self.0
            }
        }
    };
}

id_type!(WorkspaceId);
id_type!(HarnessId);
id_type!(SubagentId);

/// The kind of process a harness runs.
///
/// `Shell` is the generic escape hatch: any command can live in a harness, but
/// the agent adapters are what give us subagent telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessKind {
    ClaudeCode,
    OpenCode,
    Pi,
    Shell,
}

impl HarnessKind {
    pub fn badge(self) -> &'static str {
        match self {
            HarnessKind::ClaudeCode => "CC",
            HarnessKind::OpenCode => "OC",
            HarnessKind::Pi => "PI",
            HarnessKind::Shell => "SH",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HarnessKind::ClaudeCode => "Claude Code",
            HarnessKind::OpenCode => "opencode",
            HarnessKind::Pi => "pi",
            HarnessKind::Shell => "shell",
        }
    }

    pub fn default_command(self) -> &'static str {
        match self {
            HarnessKind::ClaudeCode => "claude",
            HarnessKind::OpenCode => "opencode",
            HarnessKind::Pi => "pi",
            HarnessKind::Shell => "$SHELL",
        }
    }

    pub const ALL: [HarnessKind; 4] = [
        HarnessKind::ClaudeCode,
        HarnessKind::OpenCode,
        HarnessKind::Pi,
        HarnessKind::Shell,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessStatus {
    /// Producing tokens or running a tool.
    Working,
    /// Waiting for input.
    Idle,
    /// Blocked on a question / permission prompt.
    NeedsInput,
    /// Exited non-zero or hit a provider error.
    Error,
}

impl HarnessStatus {
    pub fn label(self) -> &'static str {
        match self {
            HarnessStatus::Working => "working",
            HarnessStatus::Idle => "idle",
            HarnessStatus::NeedsInput => "needs input",
            HarnessStatus::Error => "error",
        }
    }
}

/// A child agent spawned by a harness (Claude Code Task, opencode child
/// session, pi equivalent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subagent {
    pub id: SubagentId,
    pub label: String,
    pub detail: String,
    pub elapsed: String,
    pub working: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Harness {
    pub id: HarnessId,
    pub workspace: WorkspaceId,
    pub name: String,
    pub kind: HarnessKind,
    /// Working directory, relative to the workspace root when possible.
    pub cwd: String,
    pub status: HarnessStatus,
    #[serde(default)]
    pub subagents: Vec<Subagent>,
    #[serde(default)]
    pub unread: u16,
    /// Short role the agent sets for itself ("frontend", "debugger").
    #[serde(default)]
    pub tag: String,
    /// One-line description of what the agent is doing, set by the agent.
    #[serde(default)]
    pub title: String,
    /// Pronouns the agent chose for itself ("she/her", "they/them").
    #[serde(default)]
    pub pronouns: String,
    /// Index into the theme accents; unset until `assign_agent_colors` runs.
    #[serde(default)]
    pub color: Option<usize>,
    /// Whether its arrival is already in the activity feed. Agents start false
    /// and announce themselves when they pick a name; shells log at once.
    /// Older state files default to true so nothing is logged twice.
    #[serde(default = "default_true")]
    pub announced: bool,
    /// Whether its first working turn is already in the activity feed.
    #[serde(default = "default_true")]
    pub started: bool,
}

fn default_true() -> bool {
    true
}

impl Harness {
    pub fn working_subagents(&self) -> usize {
        self.subagents.iter().filter(|agent| agent.working).count()
    }

    pub fn has_live_subagents(&self) -> bool {
        self.working_subagents() > 0
    }

    pub fn subagent_count(&self) -> usize {
        self.subagents.len()
    }
}

/// A workspace is a named view over a folder. Two workspaces may point at the
/// same folder, and a workspace may point at a subfolder of another project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub path: String,
    /// Index into the active theme's accent palette.
    pub accent: usize,
    pub expanded: bool,
    /// The pinned general-purpose workspace (not tied to a project folder).
    #[serde(default)]
    pub home: bool,
    /// Put away: shown in the Unloaded section, its tiles detached (the
    /// tmux sessions keep running).
    #[serde(default)]
    pub unloaded: bool,
    pub harnesses: Vec<Harness>,
}

impl Workspace {
    pub fn harness(&self, id: HarnessId) -> Option<&Harness> {
        self.harnesses.iter().find(|harness| harness.id == id)
    }

    pub fn harness_mut(&mut self, id: HarnessId) -> Option<&mut Harness> {
        self.harnesses.iter_mut().find(|harness| harness.id == id)
    }

    pub fn working(&self) -> usize {
        self.harnesses
            .iter()
            .filter(|harness| harness.status == HarnessStatus::Working)
            .count()
    }

    pub fn unread(&self) -> u16 {
        self.harnesses.iter().map(|harness| harness.unread).sum()
    }
}

/// One message in a harness mailbox. Persisted so a reload loses nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mail {
    pub to: HarnessId,
    /// Sender's display name at send time (`user` for the human or termdenctl
    /// outside a tile).
    pub from: String,
    pub text: String,
    #[serde(default)]
    pub read: bool,
}

/// One line of the activity feed. Persisted so a reload keeps the history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    /// Local wall-clock "HH:MM" when it happened.
    pub time: String,
    pub name: String,
    pub message: String,
    pub accent: usize,
    /// Recipient of a mail entry (`name → to`) and their color.
    #[serde(default)]
    pub to: Option<(String, usize)>,
    /// Harness this line is about; the feed reads its current name and color.
    #[serde(default)]
    pub harness: Option<HarnessId>,
}

/// Work an agent shows the user (images, videos, files): view only, nothing
/// is sent back. Served by the app's local results page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Showcase {
    /// Stable id in the page URL (`r/<id>/`).
    pub id: u64,
    pub from: HarnessId,
    /// The agent's name when it shared, for after it is gone.
    pub from_name: String,
    pub title: String,
    pub description: String,
    /// Wall-clock time it was shared, e.g. `14:05`.
    pub time: String,
    pub items: Vec<ShowItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowItem {
    pub path: std::path::PathBuf,
    pub caption: String,
    pub description: String,
}

/// Accent indices agents may get: the theme accents minus green (termden's own
/// status) and red (errors).
pub const AGENT_COLORS: [usize; 6] = [0, 1, 3, 4, 5, 6];

/// An agent's color follows its name (FNV-1a), so it is the same after a
/// restart or when the agent comes back. Two names can share one.
pub fn agent_color(name: &str) -> usize {
    let hash = name.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    AGENT_COLORS[(hash >> 32) as usize % AGENT_COLORS.len()]
}

pub fn assign_agent_colors(workspaces: &mut [Workspace]) {
    for harness in workspaces.iter_mut().flat_map(|w| &mut w.harnesses) {
        harness.color = Some(agent_color(&harness.name));
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn color_depends_only_on_the_name() {
        assert_eq!(agent_color("Sable"), agent_color("Sable"));
        assert!(AGENT_COLORS.contains(&agent_color("Quill")));
        let names = ["Sable", "Quill", "Tamsin", "Okonkwo", "Wren", "Bartholomew"];
        let colors: std::collections::HashSet<_> = names.iter().map(|n| agent_color(n)).collect();
        assert!(colors.len() > 1);
    }
}
