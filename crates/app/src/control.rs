//! Control socket of the running app: one line in, one reply out.
//! `termdenctl` is the client; agents use it to inspect and drive termden
//! while developing it from inside itself.
//!
//! Access control is the 0700 runtime directory around the socket.
//! shortcut: no SO_PEERCRED check yet; add it when the daemon takes over
//! the socket (roadmap step 3).

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

use alacritty_terminal::term::TermMode;
use gpui::{Context, Keystroke, Timer};
use serde_json::json;
use termden_core::state::control_socket;
use termden_core::{HarnessId, HarnessKind, WorkspaceId};

use crate::pty;
use crate::state::TermdenApp;

type Request = (String, async_channel::Sender<String>);

pub fn listen(cx: &mut Context<TermdenApp>) {
    let path = control_socket();
    if UnixStream::connect(&path).is_ok() {
        eprintln!(
            "termden: another instance owns {}; control disabled",
            path.display()
        );
        return;
    }
    let _ = fs::remove_file(&path);
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("termden: could not bind {}: {error}", path.display());
            return;
        }
    };

    let (tx, rx) = async_channel::unbounded::<Request>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || serve(stream, tx));
        }
    });

    cx.spawn(async move |this, cx| {
        while let Ok((line, reply)) = rx.recv().await {
            let out = this
                .update(cx, |this, cx| {
                    let out = this.command(&line, cx);
                    cx.notify();
                    out
                })
                .unwrap_or_else(|_| "error: app is shutting down".into());
            let _ = reply.send(out).await;
        }
    })
    .detach();
}

fn serve(stream: UnixStream, tx: async_channel::Sender<Request>) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let (reply_tx, reply_rx) = async_channel::bounded(1);
    if tx
        .send_blocking((line.trim().to_string(), reply_tx))
        .is_err()
    {
        return;
    }
    if let Ok(out) = reply_rx.recv_blocking() {
        let mut stream = stream;
        let _ = writeln!(stream, "{out}");
    }
}

pub const HELP: &str = "\
commands (<h> is a harness id or name; <ws> a workspace id or name):
  ping                       liveness check
  whoami <h>                 kind, name and tag of a harness (used by the git hook)
  state                      JSON: workspaces, harnesses, layouts, window size
  screen [h]                 visible text of a terminal (default: focused)
  history <h> [lines]        scrollback + screen, last N lines (default 200)
  send <h> <text>            type text into a harness (no Enter)
  key <h> <key>...           press keys: enter, escape, ctrl-c, up, tab, ...
  focus <h>                  focus a tile (layout and zoom stay user-only)
  add <kind> [ws]            start claude|opencode|pi|shell (default: active);
                             `wait` on it before sending, input during startup
                             can be lost
  close <h> | restart <h>    kill a harness | re-attach an exited one
  choice <json>              ask the user to pick in the browser; prints the page URL
                             {from,title,question,options:[{label,description,image}]}
  rename <h> <name>          rename a harness
  version                    running version, and a newer release if one is out
  update                     install the newer release; termden reloads into it
  title <h> [text]           set what a harness is working on (empty clears)
  tag <h> [text]             set a harness role tag (empty clears)
  pronouns <h> [text]        set a harness's pronouns (empty clears)
  add-workspace <dir>        new workspace on a folder
  activate <ws>              switch workspace
  rename-workspace <ws> <n>  rename a workspace
  mail-send <json>           deliver {from,to,text} (JSON keys, from may be 0)
  mail-read <h>              JSON: unread messages for <h>, marked read
  log <h|0> <text>           post a line to the activity feed
  settings [close]           open (or close) the settings popup
  theme                      cycle the colour theme
  reload                     save, detach, re-exec (harnesses keep running)";

impl TermdenApp {
    /// A harness by id, else by name (the active workspace wins ties).
    fn find_harness(&self, arg: Option<&str>) -> Option<HarnessId> {
        let arg = arg?;
        if let Ok(raw) = arg.parse() {
            let id = HarnessId::new(raw);
            return self.harness(id).map(|_| id);
        }
        let named =
            |w: &termden_core::Workspace| w.harnesses.iter().find(|h| h.name == arg).map(|h| h.id);
        self.workspace(self.active)
            .and_then(named)
            .or_else(|| self.workspaces.iter().find_map(named))
    }

    fn find_workspace(&self, arg: Option<&str>) -> Option<WorkspaceId> {
        let arg = arg?;
        self.workspaces
            .iter()
            .find(|w| w.id.raw().to_string() == arg || w.name == arg)
            .map(|w| w.id)
    }

    fn command(&mut self, line: &str, cx: &mut Context<Self>) -> String {
        let mut words = line.split_whitespace();
        let verb = words.next().unwrap_or("help");
        let arg = words.next();
        // Everything after the verb and its first argument, spacing intact.
        let rest = || {
            let after_verb = line.trim_start()[verb.len()..].trim_start();
            after_verb[arg.map_or(0, str::len)..]
                .trim_start()
                .to_string()
        };
        const NO_HARNESS: &str = "error: unknown harness";

        match verb {
            "ping" => "pong".into(),
            "state" => self.state_json().to_string(),
            // `shell|agent <tab> name <tab> tag`, for the git commit hook.
            "whoami" => match self.find_harness(arg).and_then(|id| self.harness(id)) {
                Some((_, h)) => format!(
                    "{}\t{}\t{}",
                    if h.kind == HarnessKind::Shell {
                        "shell"
                    } else {
                        "agent"
                    },
                    h.name,
                    h.tag
                ),
                None => NO_HARNESS.into(),
            },
            "screen" => match arg.map_or(self.focused, |a| self.find_harness(Some(a))) {
                Some(id) => match self.sessions.get(&id) {
                    Some(session) => session.screen_text(),
                    None => format!("error: harness {} has no live terminal", id.raw()),
                },
                None => NO_HARNESS.into(),
            },
            "history" => {
                let Some(id) = self.find_harness(arg) else {
                    return NO_HARNESS.into();
                };
                let lines: u32 = words.next().and_then(|n| n.parse().ok()).unwrap_or(200);
                pty::capture_history(id, lines)
            }
            "send" | "key" => {
                let Some(id) = self.find_harness(arg) else {
                    return NO_HARNESS.into();
                };
                let Some(session) = self.sessions.get_mut(&id).filter(|s| !s.exited) else {
                    return format!("error: harness {} has no live terminal", id.raw());
                };
                if verb == "send" {
                    session.input(rest().into_bytes());
                    return "ok".into();
                }
                let app_cursor = session.term.lock().mode().contains(TermMode::APP_CURSOR);
                for key in words {
                    let bytes = Keystroke::parse(key)
                        .ok()
                        .and_then(|k| pty::key_bytes(&k, app_cursor));
                    match bytes {
                        Some(bytes) => session.input(bytes),
                        None => return format!("error: unknown key {key}"),
                    }
                }
                "ok".into()
            }
            "focus" => match self.find_harness(arg) {
                Some(id) => {
                    self.focus_harness(id);
                    "ok".into()
                }
                None => NO_HARNESS.into(),
            },
            "add" => {
                let kind = match arg {
                    Some("claude") => HarnessKind::ClaudeCode,
                    Some("opencode") => HarnessKind::OpenCode,
                    Some("pi") => HarnessKind::Pi,
                    Some("shell") | None => HarnessKind::Shell,
                    Some(other) => return format!("error: unknown kind {other}"),
                };
                let ws = match words.next() {
                    Some(w) => match self.find_workspace(Some(w)) {
                        Some(ws) => ws,
                        None => return "error: unknown workspace".into(),
                    },
                    None => self.active,
                };
                match self.add_harness(ws, kind) {
                    Some(id) => {
                        // Start it now so `send` right after `add` works.
                        self.sync_sessions(cx);
                        id.raw().to_string()
                    }
                    None => "error: unknown workspace".into(),
                }
            }
            "close" | "restart" => match self.find_harness(arg) {
                Some(id) => {
                    if verb == "close" {
                        self.close_harness(id);
                    } else {
                        self.restart_harness(id);
                    }
                    "ok".into()
                }
                None => NO_HARNESS.into(),
            },
            "rename" => {
                let Some(id) = self.find_harness(arg) else {
                    return NO_HARNESS.into();
                };
                let name = rest();
                if name.is_empty() {
                    return "error: missing name".into();
                }
                self.rename_harness(id, name);
                "ok".into()
            }
            "title" => {
                let Some(id) = self.find_harness(arg) else {
                    return NO_HARNESS.into();
                };
                self.title_harness(id, rest().trim().to_string());
                "ok".into()
            }
            "tag" => {
                let Some(id) = self.find_harness(arg) else {
                    return NO_HARNESS.into();
                };
                self.tag_harness(id, rest().trim().to_string());
                "ok".into()
            }
            "pronouns" => {
                let Some(id) = self.find_harness(arg) else {
                    return NO_HARNESS.into();
                };
                self.set_pronouns(id, rest());
                "ok".into()
            }
            "add-workspace" => {
                let dir = line.trim_start_matches("add-workspace").trim();
                let path = std::path::absolute(dir).unwrap_or_else(|_| dir.into());
                if !path.is_dir() {
                    return format!("error: not a directory: {dir}");
                }
                self.add_workspace(&path).raw().to_string()
            }
            "activate" => match self.find_workspace(arg) {
                Some(ws) => {
                    self.activate_workspace(ws);
                    "ok".into()
                }
                None => "error: unknown workspace".into(),
            },
            "rename-workspace" => {
                let Some(ws) = self.find_workspace(arg) else {
                    return "error: unknown workspace".into();
                };
                let name = rest();
                if name.is_empty() {
                    return "error: missing name".into();
                }
                if let Some(workspace) = self.workspace_mut(ws) {
                    workspace.name = name;
                }
                self.save();
                "ok".into()
            }
            "mail-send" => {
                let body: serde_json::Value =
                    match serde_json::from_str(line.trim_start()["mail-send".len()..].trim()) {
                        Ok(body) => body,
                        Err(error) => return format!("error: bad json: {error}"),
                    };
                let Some(to) = self.find_harness(body["to"].as_str()) else {
                    return NO_HARNESS.into();
                };
                let from = self.find_harness(body["from"].as_str());
                let text = body["text"].as_str().unwrap_or("").trim().to_string();
                if text.is_empty() || text.len() > 20_000 {
                    return "error: text must be 1..20000 bytes".into();
                }
                if from == Some(to) {
                    return "error: cannot message yourself".into();
                }
                self.send_mail(from, to, text);
                "ok".into()
            }
            "mail-read" => match self.find_harness(arg) {
                Some(id) => {
                    let mail: Vec<_> = self
                        .read_mail(id)
                        .into_iter()
                        .map(|m| json!({ "from": m.from, "text": m.text }))
                        .collect();
                    json!(mail).to_string()
                }
                None => NO_HARNESS.into(),
            },
            "log" => {
                let text = rest();
                match self.find_harness(arg) {
                    Some(id) => self.log_harness(id, text),
                    None => self.log("Termden", text, 0),
                }
                "ok".into()
            }
            "choice" => {
                let body: serde_json::Value =
                    match serde_json::from_str(line.trim_start()["choice".len()..].trim()) {
                        Ok(body) => body,
                        Err(error) => return format!("error: bad json: {error}"),
                    };
                let Some(from) = self.find_harness(body["from"].as_str()) else {
                    return NO_HARNESS.into();
                };
                let Some(url) = crate::choices::url() else {
                    return "error: choice server is not running".into();
                };
                match crate::choices::parse(from, &body) {
                    Ok(choice) => {
                        self.add_choice(choice);
                        url
                    }
                    Err(error) => format!("error: {error}"),
                }
            }
            "settings" => {
                self.settings_open = arg != Some("close");
                "ok".into()
            }
            "theme" => {
                self.cycle_theme();
                format!("{:?}", self.config.theme).to_lowercase()
            }
            "version" => match &self.update_available {
                Some(tag) => format!("{} ({tag} available)", crate::update::VERSION),
                None => crate::update::VERSION.into(),
            },
            "update" => self.apply_update(cx),
            "reload" => {
                // Reply first, then exec.
                cx.spawn(async move |this, cx| {
                    Timer::after(Duration::from_millis(100)).await;
                    let _ = this.update(cx, |this, _| this.reload());
                })
                .detach();
                "reloading".into()
            }
            _ => HELP.into(),
        }
    }

    fn state_json(&self) -> serde_json::Value {
        let workspaces: Vec<_> = self
            .workspaces
            .iter()
            .map(|w| {
                let harnesses: Vec<_> = w
                    .harnesses
                    .iter()
                    .map(|h| {
                        let session = self.sessions.get(&h.id);
                        json!({
                            "id": h.id,
                            "name": h.name,
                            "kind": h.kind,
                            "status": h.status,
                            "cwd": h.cwd,
                            "tmux": format!("td-{}", h.id.raw()),
                            "live": session.is_some_and(|s| !s.exited),
                            "unread": h.unread,
                            "tag": h.tag,
                            "pronouns": h.pronouns,
                            "agent_title": h.title,
                            "title": session.map(|s| s.title.clone()),
                            "grid": session.map(|s| [s.cols, s.rows]),
                        })
                    })
                    .collect();
                json!({
                    "id": w.id,
                    "name": w.name,
                    "path": w.path,
                    "harnesses": harnesses,
                    "layout": self.layouts.get(&w.id),
                })
            })
            .collect();
        json!({
            "pid": std::process::id(),
            "uptime_s": self.started.elapsed().as_secs_f32(),
            "mode": format!("{:?}", self.mode).to_lowercase(),
            "window": { "width": self.viewport.0, "height": self.viewport.1 },
            "theme": self.config.theme,
            "active": self.active,
            "focused": self.focused,
            "zoomed": self.zoomed,
            "tmux_socket": pty::tmux_socket(),
            "workspaces": workspaces,
        })
    }
}
