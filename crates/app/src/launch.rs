//! Per-harness launch configuration: tells Claude Code and opencode they are
//! inside termden and hands them the termden MCP server. Everything is passed
//! per process (flags, env, files in our runtime dir), never global dotfiles.

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use termden_core::state::{control_socket, runtime_dir};
use termden_core::{HarnessId, HarnessKind};

const HOME_PROMPT: &str = " You are the Home agent: you belong to no project. Your job is creating workspaces and general conversation with the user. To make a workspace run `termdenctl add-workspace <dir>` (create the folder first if needed; it prints the workspace id); start an agent there with `termdenctl add claude <workspace id>` only if the user wants one. Do not do project work yourself: point the user to the workspace's agents.";

const MEMORY_PROMPT: &str = " The rhodon MCP server is this project's persistent memory (tools list, read, write, append, search, delete, compact). It is already scoped to this project. At the start of a task, read what is relevant; when you learn something a future agent would need (decisions, gotchas, the user's preferences), write it down as a short note. Keep it tidy: call compact now and then.";

/// Rhodon project folder for a workspace: its folder name, if rhodon-mcp is installed.
pub fn memory_project(path: &str, home: bool) -> Option<String> {
    let name = std::path::Path::new(path).file_name()?.to_str()?;
    let ok = !home && !name.starts_with('.') && !name.contains(['/', '\\', '*']);
    let installed = std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("rhodon-mcp").is_file()));
    (ok && installed).then(|| name.to_string())
}

fn prompt(id: HarnessId, name: &str, home: bool, memory: bool) -> String {
    let text = format!(
        "You are running inside termden as harness {} (named {name}). Other agents run in tiles \
         beside you. The termden MCP server gives you tools (in Claude Code they are named \
         mcp__termden__<tool>; if they are deferred, load them with ToolSearch first, and never \
         conclude they do not exist without trying): termden_agents lists agents, \
         termden_send_mail messages one, termden_read_mail reads your inbox, termden_read_screen \
         shows what another agent is doing, termden_set_tag / termden_set_title \
         describe your role and current work. Before anything else, even if your very first message is a task and even if it seems unrelated, call termden_set_tag with a one-word role (frontend, backend, debugger...). Keep termden_set_title current with a short line about \
         what you are doing. To ask the user to choose between options, use your own question tool in the terminal; when the options are images or files, first call termden_show_results with captions that match the option labels (like A and B), post its link, then ask. When you produce images, screenshots, renders, videos or other \
         files the user should look at, call termden_show_results with their absolute paths and post \
         the link it returns. When a line starts with 📬 (a mail preview from termden), call \
         termden_read_mail. To start a helper agent run `termdenctl add claude` (or opencode); it \
         prints the new harness id N. Then run `termdenctl wait N 20` (it needs a few seconds \
         before it accepts input), give it its task with `termdenctl send --enter N \"task\"` and \
         check with `termdenctl screen N` that the text arrived; never target `self`/your own id. \
         Git commits from your tile are refused until you have a tag; the commit hook adds the closing line `> Written by <name> [<tag>].` itself, so never write your own sign-off or co-author line, and keep subjects clean (type(scope): summary). Do not type into other agents terminals except helpers you started, and do not move or \
         resize tiles.",
        id.raw(),
    );
    let text = if memory { text + MEMORY_PROMPT } else { text };
    if home { text + HOME_PROMPT } else { text }
}

/// Shell command line for a new harness; written files live under
/// `$XDG_RUNTIME_DIR/termden/harness/<id>/`.
pub fn command(
    id: HarnessId,
    name: &str,
    kind: HarnessKind,
    home: bool,
    memory: Option<&str>,
) -> String {
    let base = kind.default_command();
    if !matches!(kind, HarnessKind::ClaudeCode | HarnessKind::OpenCode) {
        return base.into();
    }
    match write_config(id, name, kind, home, memory) {
        Some(command) => command,
        None => {
            eprintln!("termden: could not write launch config; starting {base} plain");
            base.into()
        }
    }
}

fn write_config(
    id: HarnessId,
    name: &str,
    kind: HarnessKind,
    home: bool,
    memory: Option<&str>,
) -> Option<String> {
    let dir: PathBuf = runtime_dir().join("harness").join(id.raw().to_string());
    fs::create_dir_all(&dir).ok()?;
    let ctl = crate::pty::bin_dir()?.join("termdenctl");
    let env = json!({
        "TERMDEN_HARNESS": id.raw().to_string(),
        "TERMDEN_CONTROL_SOCKET": control_socket(),
    });
    let q = |p: &PathBuf| format!("'{}'", p.display());
    match kind {
        HarnessKind::ClaudeCode => {
            let mcp = dir.join("mcp.json");
            let mut config = json!({ "mcpServers": { "termden": {
                "command": ctl, "args": ["mcp"], "env": env,
            }}});
            if let Some(project) = memory {
                config["mcpServers"]["rhodon"] =
                    json!({ "command": ctl, "args": ["memory", project] });
            }
            fs::write(&mcp, config.to_string()).ok()?;
            // The commit hook adds the sign-off; Claude must not add its own.
            let settings = dir.join("settings.json");
            // Claude reports its session id on start, `/clear` and `--resume`;
            // the stats pane finds the transcript through it.
            let hook = format!("{} session-hook {}", q(&ctl), q(&dir.join("session.id")));
            // The status line input carries plan limits and context fill;
            // `termdenctl statusline` keeps a copy and runs the user's own line.
            let status = format!("{} statusline {}", q(&ctl), q(&dir.join("status.json")));
            let body = json!({
                "attribution": { "commit": "", "pr": "", "sessionUrl": false },
                "hooks": { "SessionStart": [{ "hooks": [{ "type": "command", "command": hook }] }] },
                "statusLine": { "type": "command", "command": status },
            });
            fs::write(&settings, body.to_string()).ok()?;
            Some(format!(
                "claude --settings {} --mcp-config {} --append-system-prompt '{}'",
                q(&settings),
                q(&mcp),
                prompt(id, name, home, memory.is_some()).replace('\'', "")
            ))
        }
        _ => {
            let notes = dir.join("instructions.md");
            fs::write(&notes, prompt(id, name, home, memory.is_some())).ok()?;
            let config = dir.join("opencode.json");
            let mut body = json!({
                "instructions": [notes],
                "mcp": { "termden": {
                    "type": "local",
                    "command": [ctl, "mcp"],
                    "environment": env,
                    "enabled": true,
                }},
            });
            if let Some(project) = memory {
                body["mcp"]["rhodon"] = json!({
                    "type": "local", "command": [ctl, "memory", project], "enabled": true,
                });
            }
            fs::write(&config, body.to_string()).ok()?;
            Some(format!(
                "OPENCODE_CONFIG={} opencode --standalone",
                q(&config)
            ))
        }
    }
}
