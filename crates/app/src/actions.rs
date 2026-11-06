//! Things that change what exists: workspaces, harnesses, the saved state,
//! and the running binary itself (reload on rebuild).

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::time::Duration;

use termden_core::{
    AppState, DropZone, Harness, HarnessId, HarnessKind, HarnessStatus, LayoutNode, Workspace,
    WorkspaceId,
};

use crate::pty;
use crate::state::{Activity, Mode, TermdenApp};

/// No saved state yet: one workspace on the launch directory, one shell.
pub fn first_run() -> AppState {
    let dir = std::env::current_dir()
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(Into::into))
        .unwrap_or_else(|| "/".into());
    let mut state = AppState::default();
    new_workspace(&mut state, &dir);
    let ws = state.workspaces[0].id;
    let id = state.next_harness_id();
    state.workspaces[0]
        .harnesses
        .push(new_harness(id, ws, HarnessKind::Shell, &[]));
    state.layouts.insert(ws, LayoutNode::leaf(id));
    state.active = Some(ws);
    state.focused = Some(id);
    state
}

fn new_workspace(state: &mut AppState, dir: &Path) -> WorkspaceId {
    let id = state.next_workspace_id();
    let name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    state.workspaces.push(Workspace {
        id,
        name,
        path: dir.display().to_string(),
        accent: state.workspaces.len() % 8,
        expanded: true,
        harnesses: Vec::new(),
    });
    id
}

fn new_harness(id: HarnessId, ws: WorkspaceId, kind: HarnessKind, siblings: &[Harness]) -> Harness {
    let base = match kind {
        HarnessKind::ClaudeCode => "Claude",
        HarnessKind::Shell => "Shell",
        other => other.label(),
    };
    let taken = siblings.iter().filter(|h| h.kind == kind).count();
    Harness {
        id,
        workspace: ws,
        name: if taken == 0 {
            base.to_string()
        } else {
            format!("{base} {}", taken + 1)
        },
        kind,
        cwd: ".".into(),
        status: HarnessStatus::Idle,
        subagents: Vec::new(),
        unread: 0,
        tag: String::new(),
        title: String::new(),
        pronouns: String::new(),
    }
}

impl TermdenApp {
    fn snapshot(&self) -> AppState {
        AppState {
            workspaces: self.workspaces.clone(),
            layouts: self.layouts.clone(),
            active: Some(self.active),
            focused: self.focused,
            mail: self.mail.clone(),
            activity: self.activity.clone(),
            choices: self.choices.clone(),
            choice_open: self.choice_open,
        }
    }

    /// Persist after every structural change. Only the Live instance writes.
    pub fn save(&self) {
        if self.mode != Mode::Live {
            return;
        }
        if let Err(error) = self.snapshot().save() {
            eprintln!("termden: could not save state: {error}");
        }
    }

    pub fn log(&mut self, name: impl Into<String>, message: String, accent: usize) {
        self.activity.push(Activity {
            time: crate::state::clock(0),
            name: name.into(),
            message,
            accent,
        });
        if self.activity.len() > 200 {
            self.activity.remove(0);
        }
        self.activity_stick.set(true);
        self.save();
    }

    pub fn log_harness(&mut self, id: HarnessId, message: String) {
        if let Some((workspace, harness)) = self.harness(id) {
            let (name, accent) = (harness.name.clone(), workspace.accent);
            self.log(name, message, accent);
        }
    }

    pub fn add_workspace(&mut self, dir: &Path) -> WorkspaceId {
        let mut state = self.snapshot();
        let id = new_workspace(&mut state, dir);
        self.workspaces = state.workspaces;
        let folder = dir.file_name().map_or_else(
            || dir.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        self.log("Workspace", format!("{folder} added"), 0);
        self.activate_workspace(id);
        self.add_harness(id, HarnessKind::Shell);
        id
    }

    /// New harness in `ws`, split off the focused tile (dwindle: along the
    /// tile's longer side).
    pub fn add_harness(&mut self, ws: WorkspaceId, kind: HarnessKind) -> Option<HarnessId> {
        let id = self.snapshot().next_harness_id();
        let workspace = self.workspace_mut(ws)?;
        let harness = new_harness(id, ws, kind, &workspace.harnesses);
        workspace.harnesses.push(harness);
        workspace.expanded = true;

        let target = self
            .focused
            .filter(|f| self.layouts.get(&ws).is_some_and(|l| l.contains(*f)))
            .or_else(|| self.layouts.get(&ws).and_then(|l| l.first_leaf()));
        let rects = self.layout_tiles(ws);
        self.zoomed = None;
        let zone = target
            .and_then(|t| rects.iter().find(|(id, _)| *id == t))
            .map(|(_, rect)| {
                if rect.w >= rect.h * 1.2 {
                    DropZone::Right
                } else {
                    DropZone::Bottom
                }
            })
            .unwrap_or(DropZone::Right);
        match (self.layouts.get_mut(&ws), target) {
            (Some(layout), Some(target)) => {
                layout.split_at(target, id, zone);
            }
            _ => {
                self.layouts.insert(ws, LayoutNode::leaf(id));
            }
        }

        self.adding = None;
        self.log_harness(id, "joined".into());
        self.focus_harness(id);
        Some(id)
    }

    /// Close a tile for good: the tmux session is killed.
    pub fn close_harness(&mut self, id: HarnessId) {
        let Some(ws) = self.harness(id).map(|(w, _)| w.id) else {
            return;
        };
        self.log_harness(id, "left".into());
        self.mail.retain(|m| m.to != id);
        self.sessions.remove(&id);
        pty::kill_tmux_session(id);
        if let Some(workspace) = self.workspace_mut(ws) {
            workspace.harnesses.retain(|h| h.id != id);
        }
        let empty = match self.layouts.get_mut(&ws) {
            Some(layout) => !layout.remove(id) && layout.contains(id),
            None => false,
        };
        if empty {
            self.layouts.remove(&ws);
        }
        if self.focused == Some(id) {
            self.focused = self.layouts.get(&ws).and_then(|l| l.first_leaf());
        }
        if self.zoomed == Some(id) {
            self.zoomed = None;
        }
        self.save();
    }

    pub fn remove_workspace(&mut self, ws: WorkspaceId) {
        let ids: Vec<HarnessId> = self
            .workspace(ws)
            .map(|w| w.harnesses.iter().map(|h| h.id).collect())
            .unwrap_or_default();
        for id in ids {
            self.close_harness(id);
        }
        if let Some(workspace) = self.workspace(ws) {
            let name = workspace.name.clone();
            self.log("Workspace", format!("{name} removed"), 0);
        }
        self.workspaces.retain(|w| w.id != ws);
        self.layouts.remove(&ws);
        self.confirm_remove = None;
        if self.active == ws {
            match self.workspaces.first().map(|w| w.id) {
                Some(next) => self.activate_workspace(next),
                None => self.focused = None,
            }
        }
        self.save();
    }

    pub fn rename_harness(&mut self, id: HarnessId, name: String) {
        if let Some(harness) = self
            .workspaces
            .iter_mut()
            .find_map(|workspace| workspace.harness_mut(id))
        {
            let mut chars = name.chars();
            harness.name = chars
                .next()
                .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect());
        }
        self.save();
    }

    pub fn title_harness(&mut self, id: HarnessId, title: String) {
        if let Some(harness) = self
            .workspaces
            .iter_mut()
            .find_map(|workspace| workspace.harness_mut(id))
        {
            let text = title.trim_start_matches(|c: char| !c.is_alphanumeric());
            harness.title = if text.is_empty() {
                String::new()
            } else {
                format!("✳ {}", text.chars().take(60).collect::<String>())
            };
        }
        self.save();
    }

    /// `/clear` wipes the agent's memory of who it was: back to the default
    /// name, no tag, no title (it names itself again from its system prompt).
    pub fn reset_identity(&mut self, id: HarnessId) {
        let Some(ws) = self.workspaces.iter_mut().find(|w| w.harness(id).is_some()) else {
            return;
        };
        let (kind, siblings): (_, Vec<Harness>) = {
            let h = ws.harness(id).unwrap();
            (
                h.kind,
                ws.harnesses
                    .iter()
                    .filter(|o| o.id != id)
                    .cloned()
                    .collect(),
            )
        };
        let fresh = new_harness(id, ws.id, kind, &siblings);
        if let Some(h) = ws.harness_mut(id) {
            h.name = fresh.name;
            h.tag.clear();
            h.title.clear();
            h.pronouns.clear();
        }
        self.save();
    }

    pub fn tag_harness(&mut self, id: HarnessId, tag: String) {
        if let Some(harness) = self
            .workspaces
            .iter_mut()
            .find_map(|workspace| workspace.harness_mut(id))
        {
            harness.tag = tag.chars().take(12).collect();
        }
        self.save();
    }

    pub fn set_pronouns(&mut self, id: HarnessId, pronouns: String) {
        if let Some(harness) = self
            .workspaces
            .iter_mut()
            .find_map(|workspace| workspace.harness_mut(id))
        {
            harness.pronouns = pronouns.trim().to_lowercase().chars().take(16).collect();
        }
        self.save();
    }

    /// Re-attach an exited tile (tmux makes a fresh session if it is gone).
    pub fn restart_harness(&mut self, id: HarnessId) {
        self.sessions.remove(&id);
        self.set_status(id, HarnessStatus::Idle);
        self.log_harness(id, "was restarted".into());
    }

    /// Called about once a second: a successful `cargo build` replaced our
    /// binary, so reload into it.
    pub fn check_rebuilt(&mut self) {
        let Some((path, mtime)) = &self.exe else {
            return;
        };
        let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) else {
            return;
        };
        // Wait until the linker has been done for a moment.
        let settled = modified
            .elapsed()
            .is_ok_and(|age| age > Duration::from_millis(700));
        if modified != *mtime && settled {
            self.reload();
        }
    }

    /// Hot reload: save, detach every tmux client, exec the current binary.
    /// Harnesses keep running in tmux and are reattached by the new process.
    pub fn reload(&mut self) {
        let Some(path) = self
            .exe
            .as_ref()
            .map(|(path, _)| path.clone())
            .or_else(|| std::env::current_exe().ok())
        else {
            return;
        };
        self.save();
        for (_, session) in self.sessions.drain() {
            session.shutdown_and_wait();
        }
        let error = std::process::Command::new(&path)
            .args(std::env::args_os().skip(1))
            .env("TERMDEN_RELOADED", "1")
            .exec();
        // Only reached if exec failed; sessions respawn on the next frame.
        eprintln!("termden: reload failed: {error}");
        self.log("Termden", "could not update itself".into(), 7);
        self.exe = None;
    }
}
