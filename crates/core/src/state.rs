//! Persisted session state: which workspaces exist, their harnesses and tile
//! layouts. Harness processes themselves live in tmux and outlive the app;
//! this file is what lets a restarted (or hot-reloaded) UI find them again.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    Activity, Choice, HarnessId, HarnessStatus, LayoutNode, Mail, Showcase, Workspace, WorkspaceId,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub workspaces: Vec<Workspace>,
    pub layouts: HashMap<WorkspaceId, LayoutNode>,
    pub active: Option<WorkspaceId>,
    pub focused: Option<HarnessId>,
    pub mail: Vec<Mail>,
    pub activity: Vec<Activity>,
    pub choices: Vec<Choice>,
    pub showcases: Vec<Showcase>,
}

impl AppState {
    /// `$TERMDEN_STATE_FILE`, else `$XDG_STATE_HOME/termden/state.json`.
    pub fn path() -> PathBuf {
        match env::var("TERMDEN_STATE_FILE") {
            Ok(path) if !path.is_empty() => PathBuf::from(path),
            _ => state_dir().join("state.json"),
        }
    }

    /// Missing file means a fresh start; a corrupt one is kept aside rather
    /// than overwritten, so a bad write never silently loses workspaces.
    pub fn load() -> Option<Self> {
        let path = Self::path();
        let text = fs::read_to_string(&path).ok()?;
        match serde_json::from_str::<Self>(&text) {
            Ok(mut state) => {
                // Live statuses are re-derived from the running sessions.
                for harness in state
                    .workspaces
                    .iter_mut()
                    .flat_map(|w| w.harnesses.iter_mut())
                {
                    harness.status = HarnessStatus::Idle;
                }
                Some(state)
            }
            Err(error) => {
                let backup = path.with_extension("json.corrupt");
                eprintln!(
                    "termden: could not parse {} ({error}); moved to {}",
                    path.display(),
                    backup.display()
                );
                let _ = fs::rename(&path, backup);
                None
            }
        }
    }

    /// Atomic write: temp file + rename.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(&tmp, text)?;
        fs::rename(tmp, path)
    }

    pub fn next_harness_id(&self) -> HarnessId {
        // Activity lines remember the harness they are about. Never hand their
        // id to a newcomer: an old "so-and-so left" line would then be painted
        // in the new agent's colour.
        let max = self
            .workspaces
            .iter()
            .flat_map(|w| w.harnesses.iter())
            .map(|h| h.id.raw())
            .chain(
                self.activity
                    .iter()
                    .filter_map(|entry| entry.harness.map(|id| id.raw())),
            )
            .max()
            .unwrap_or(0);
        HarnessId::new(max + 1)
    }

    pub fn next_workspace_id(&self) -> WorkspaceId {
        let max = self
            .workspaces
            .iter()
            .map(|w| w.id.raw())
            .max()
            .unwrap_or(0);
        WorkspaceId::new(max + 1)
    }
}

pub fn state_dir() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state").join("termden")
}

/// `$XDG_RUNTIME_DIR/termden`, home of the control socket.
pub fn runtime_dir() -> PathBuf {
    match env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir).join("termden"),
        _ => env::temp_dir().join(format!("termden-{}", env::var("USER").unwrap_or_default())),
    }
}

/// Control socket the running app listens on. Overridable for tests.
pub fn control_socket() -> PathBuf {
    match env::var("TERMDEN_CONTROL_SOCKET") {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => runtime_dir().join("app.sock"),
    }
}

pub(crate) fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    if let Ok(dir) = env::var(var) {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    match env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(fallback),
        Err(_) => PathBuf::from(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Axis, Harness, HarnessKind};

    #[test]
    fn round_trips_through_json() {
        let ws = WorkspaceId::new(1);
        let mut state = AppState {
            workspaces: vec![Workspace {
                id: ws,
                name: "w".into(),
                path: "/tmp".into(),
                accent: 0,
                expanded: true,
                harnesses: vec![Harness {
                    id: HarnessId::new(4),
                    workspace: ws,
                    name: "shell".into(),
                    kind: HarnessKind::Shell,
                    cwd: ".".into(),
                    status: HarnessStatus::Working,
                    subagents: vec![],
                    unread: 0,
                    tag: String::new(),
                    title: String::new(),
                    pronouns: String::new(),
                    color: None,
                    announced: true,
                    started: true,
                }],
            }],
            active: Some(ws),
            focused: Some(HarnessId::new(4)),
            ..Default::default()
        };
        state.layouts.insert(
            ws,
            LayoutNode::split(
                Axis::Vertical,
                0.4,
                LayoutNode::leaf(HarnessId::new(4)),
                LayoutNode::leaf(HarnessId::new(5)),
            ),
        );

        let text = serde_json::to_string(&state).unwrap();
        let back: AppState = serde_json::from_str(&text).unwrap();
        assert_eq!(
            back.layouts[&ws].leaves(),
            [HarnessId::new(4), HarnessId::new(5)]
        );
        assert_eq!(back.next_harness_id(), HarnessId::new(5));
        assert_eq!(back.next_workspace_id(), WorkspaceId::new(2));
    }

    #[test]
    fn ids_are_not_reused_while_activity_still_remembers_them() {
        // A departed agent keeps a line in the feed. Handing its id to the
        // next agent would repaint that line in the newcomer's colour.
        let state = AppState {
            activity: vec![Activity {
                time: "00:00".into(),
                name: "gone".into(),
                message: "left".into(),
                accent: 0,
                to: None,
                harness: Some(HarnessId::new(7)),
            }],
            ..Default::default()
        };
        assert_eq!(state.next_harness_id(), HarnessId::new(8));
    }
}
