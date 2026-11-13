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

/// "joined", "joined · backend", "joined · backend · she/her".
pub(crate) fn join_message(harness: &Harness) -> String {
    let mut details = Vec::new();
    if !harness.tag.is_empty() {
        details.push(harness.tag.clone());
    }
    if !harness.pronouns.is_empty() {
        details.push(harness.pronouns.clone());
    }
    if details.is_empty() {
        "joined".into()
    } else {
        format!("joined · {}", details.join(" · "))
    }
}

/// True for the auto-generated names of agent harnesses ("opencode",
/// "Claude 2", "pi 3"). Shells are not included: "Shell joined" is real.
fn is_placeholder_name(name: &str) -> bool {
    ["Claude", "opencode", "pi"].iter().any(|base| {
        name == *base
            || name.strip_prefix(base).is_some_and(|rest| {
                rest.strip_prefix(' ')
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            })
    })
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
        color: None,
        // Shells never pick a name, so announce them right away. Agents wait
        // until they say who they are (or start working without saying).
        announced: kind == HarnessKind::Shell,
        started: kind == HarnessKind::Shell,
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
            showcases: self.showcases.clone(),
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
        self.push_activity(Activity {
            time: crate::state::clock(0),
            name: name.into(),
            message,
            accent,
            to: None,
            harness: None,
        });
    }

    pub fn log_harness(&mut self, id: HarnessId, message: String) {
        if let Some((workspace, harness)) = self.harness(id) {
            let (name, accent) = (
                harness.name.clone(),
                harness.color.unwrap_or(workspace.accent),
            );
            self.push_activity(Activity {
                time: crate::state::clock(0),
                name,
                message,
                accent,
                to: None,
                harness: Some(id),
            });
        }
    }

    fn push_activity(&mut self, entry: Activity) {
        self.activity.push(entry);
        if self.activity.len() > 200 {
            self.activity.remove(0);
        }
        self.activity_stick.set(true);
        self.save();
    }

    /// Older builds wrote a placeholder `"<kind> <n> joined"` line at spawn,
    /// before the agent had a name. Those lines have no harness id and cannot
    /// be reassociated, so drop them on load; the agent's real join line
    /// appears when it announces itself.
    pub(crate) fn drop_placeholder_joins(&mut self) {
        self.activity.retain(|entry| {
            entry.harness.is_some()
                || entry.message != "joined"
                || !is_placeholder_name(&entry.name)
        });
    }

    fn join_entry_mut(&mut self, id: HarnessId, old_name: Option<&str>) -> Option<&mut Activity> {
        self.activity.iter_mut().rev().find(|entry| {
            entry.message.starts_with("joined")
                && (entry.harness == Some(id)
                    || (entry.harness.is_none() && old_name.is_some_and(|name| entry.name == name)))
        })
    }

    /// Rewrite the join line for `id`; returns false when there is none.
    fn touch_join_entry(
        &mut self,
        id: HarnessId,
        old_name: Option<&str>,
        name: String,
        message: Option<String>,
        accent: usize,
    ) -> bool {
        let Some(entry) = self.join_entry_mut(id, old_name) else {
            return false;
        };
        let mut changed = false;
        if entry.name != name {
            entry.name = name;
            changed = true;
        }
        if let Some(message) = message
            && entry.message != message
        {
            entry.message = message;
            changed = true;
        }
        if entry.harness != Some(id) {
            entry.harness = Some(id);
            changed = true;
        }
        if entry.accent != accent {
            entry.accent = accent;
            changed = true;
        }
        if changed {
            self.save();
        }
        true
    }

    /// An agent got its name: log its arrival (once) and keep that one line in
    /// step with the tag and pronouns until it starts working.
    pub(crate) fn announce_name(&mut self, id: HarnessId) {
        let Some((workspace, harness)) = self.harness(id) else {
            return;
        };
        if harness.kind == HarnessKind::Shell {
            return;
        }
        let name = harness.name.clone();
        let message = join_message(harness);
        let accent = harness.color.unwrap_or(workspace.accent);
        let (announced, started) = (harness.announced, harness.started);
        if !announced {
            if let Some(harness) = self
                .workspaces
                .iter_mut()
                .find_map(|workspace| workspace.harness_mut(id))
            {
                harness.announced = true;
            }
            if !self.touch_join_entry(id, None, name, Some(message.clone()), accent) {
                self.log_harness(id, message);
            }
        } else {
            // Older state files had no `announced`; a later rename still fixes
            // a placeholder line that was logged under the default name.
            let message = (!started).then(|| join_message(harness));
            self.touch_join_entry(id, None, name, message, accent);
        } // The agent may already be running when it introduces itself.
        let working = self
            .harness(id)
            .is_some_and(|(_, harness)| harness.status == HarnessStatus::Working);
        if working && !started {
            if let Some(harness) = self
                .workspaces
                .iter_mut()
                .find_map(|workspace| workspace.harness_mut(id))
            {
                harness.started = true;
            }
            self.log_harness(id, "started working".into());
        }
    }

    /// Tag and pronouns usually arrive right after the name. Rewrite the join
    /// line instead of stacking another row; never flip `announced` here.
    pub(crate) fn refresh_join(&mut self, id: HarnessId) {
        let Some((workspace, harness)) = self.harness(id) else {
            return;
        };
        if harness.kind == HarnessKind::Shell || !harness.announced || harness.started {
            return;
        }
        let name = harness.name.clone();
        let message = join_message(harness);
        let accent = harness.color.unwrap_or(workspace.accent);
        self.touch_join_entry(id, None, name, Some(message), accent);
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
        termden_core::assign_agent_colors(&mut self.workspaces, crate::state::color_seed());

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
        // Agents wait until they name themselves; shells are announced now.
        if kind == HarnessKind::Shell {
            self.log_harness(id, "joined".into());
        }
        self.focus_harness(id);
        Some(id)
    }

    /// Close a tile for good: the tmux session is killed.
    pub fn close_harness(&mut self, id: HarnessId) {
        let Some((workspace, harness)) = self.harness(id) else {
            return;
        };
        let ws = workspace.id;
        // An agent that never introduced itself never joined; do not log a
        // departure for a name the panel still showed as "starting…".
        if harness.announced {
            self.log_harness(id, "left".into());
        }
        self.mail.retain(|m| m.to != id);
        self.mail_notified.remove(&id);
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
        let mut old_name = None;
        if let Some(harness) = self
            .workspaces
            .iter_mut()
            .find_map(|workspace| workspace.harness_mut(id))
        {
            old_name = Some(harness.name.clone());
            let mut chars = name.chars();
            harness.name = chars
                .next()
                .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect());
        }
        if let Some(old_name) = old_name
            && let Some((workspace, harness)) = self.harness(id)
        {
            let name = harness.name.clone();
            let message = (!harness.started).then(|| join_message(harness));
            let accent = harness.color.unwrap_or(workspace.accent);
            self.touch_join_entry(id, Some(&old_name), name, message, accent);
        }
        self.announce_name(id);
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
        self.refresh_join(id);
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
        self.refresh_join(id);
        self.save();
    }

    /// Re-attach an exited tile (tmux makes a fresh session if it is gone).
    pub fn restart_harness(&mut self, id: HarnessId) {
        self.sessions.remove(&id);
        self.set_status(id, HarnessStatus::Idle);
        self.log_harness(id, "was restarted".into());
    }

    /// Called about once a second: when a `cargo build` replaced our binary,
    /// offer a restart. Never restart unasked: the user may be mid-task.
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
            let path = path.clone();
            self.exe = Some((path, modified));
            self.request_restart("New build ready");
        }
    }

    /// Show the restart banner; the user confirms with its button or
    /// ctrl+shift+r. Agents (`termdenctl reload`) can only ask.
    pub fn request_restart(&mut self, reason: &str) {
        self.restart_pending = Some(reason.to_string());
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

#[cfg(test)]
mod tests {
    use super::is_placeholder_name;

    #[test]
    fn placeholder_names_are_kind_labels_and_numbers() {
        for name in [
            "opencode",
            "opencode 2",
            "Claude",
            "Claude 12",
            "pi",
            "pi 3",
        ] {
            assert!(is_placeholder_name(name), "{name}");
        }
        for name in ["Wren", "Juno", "Shell", "opencode dev", "Claude Code"] {
            assert!(!is_placeholder_name(name), "{name}");
        }
    }
}
