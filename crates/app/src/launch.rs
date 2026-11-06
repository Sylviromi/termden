//! Per-harness launch configuration: tells Claude Code and opencode they are
//! inside termden and hands them the termden MCP server. Everything is passed
//! per process (flags, env, files in our runtime dir), never global dotfiles.

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use termden_core::state::{control_socket, runtime_dir};
use termden_core::{HarnessId, HarnessKind};

fn prompt(id: HarnessId, name: &str) -> String {
    format!(
        "You are running inside termden as harness {} (named {name}). Other agents run in tiles \
         beside you. The termden MCP server gives you tools (in Claude Code they are named \
         mcp__termden__<tool>; if they are deferred, load them with ToolSearch first, and never \
         conclude they do not exist without trying): termden_agents lists agents, \
         termden_send_mail messages one, termden_read_mail reads your inbox, termden_read_screen \
         shows what another agent is doing, termden_set_name / termden_set_tag / termden_set_pronouns / termden_set_title \
         name you and describe your role and current work. Before anything else, even if your very first message is a task and even if it seems \
         unrelated, call termden_set_name with a short unique name (not the default kind name) and \
         termden_set_tag with a one-word role (frontend, backend, debugger...), and termden_set_pronouns with the pronouns you choose for yourself (shown next to your name); keep all unless \
         the user asks you to change them. Keep termden_set_title current with a short line about \
         what you are doing. IMPORTANT: whenever you have options for the user to choose between \
         (design variants, logos, approaches, names) or the user asks you to show them options, \
         call termden_ask_user_choice (2-8 options, images as absolute png/jpg/svg paths) so a \
         popup appears for them; do not just write files or describe options in text. Their pick \
         comes back as mail from user. When a line says `[termden] New message`, call \
         termden_read_mail. To start a helper agent run `termdenctl add claude` (or opencode); it \
         prints the new harness id N. Then run `termdenctl wait N 20` (it needs a few seconds \
         before it accepts input), give it its task with `termdenctl send --enter N \"task\"` and \
         check with `termdenctl screen N` that the text arrived; never target `self`/your own id. \
         Git commits from your tile are refused until you have a name and a tag; the commit hook adds the closing line `Written by <name> [<tag>].` itself, so never write your own sign-off or co-author line, and keep subjects clean (type(scope): summary). Do not type into other agents terminals except helpers you started, and do not move or \
         resize tiles.",
        id.raw(),
    )
}

/// Shell command line for a new harness; written files live under
/// `$XDG_RUNTIME_DIR/termden/harness/<id>/`.
pub fn command(id: HarnessId, name: &str, kind: HarnessKind) -> String {
    let base = kind.default_command();
    if !matches!(kind, HarnessKind::ClaudeCode | HarnessKind::OpenCode) {
        return base.into();
    }
    match write_config(id, name, kind) {
        Some(command) => command,
        None => {
            eprintln!("termden: could not write launch config; starting {base} plain");
            base.into()
        }
    }
}

fn write_config(id: HarnessId, name: &str, kind: HarnessKind) -> Option<String> {
    let dir: PathBuf = runtime_dir().join("harness").join(id.raw().to_string());
    fs::create_dir_all(&dir).ok()?;
    let ctl = std::env::current_exe().ok()?.with_file_name("termdenctl");
    let env = json!({
        "TERMDEN_HARNESS": id.raw().to_string(),
        "TERMDEN_CONTROL_SOCKET": control_socket(),
    });
    let q = |p: &PathBuf| format!("'{}'", p.display());
    match kind {
        HarnessKind::ClaudeCode => {
            let mcp = dir.join("mcp.json");
            let config = json!({ "mcpServers": { "termden": {
                "command": ctl, "args": ["mcp"], "env": env,
            }}});
            fs::write(&mcp, config.to_string()).ok()?;
            // The commit hook adds the sign-off; Claude must not add its own.
            let settings = dir.join("settings.json");
            let attribution =
                json!({ "attribution": { "commit": "", "pr": "", "sessionUrl": false }});
            fs::write(&settings, attribution.to_string()).ok()?;
            Some(format!(
                "claude --settings {} --mcp-config {} --append-system-prompt '{}'",
                q(&settings),
                q(&mcp),
                prompt(id, name).replace('\'', "")
            ))
        }
        _ => {
            let notes = dir.join("instructions.md");
            fs::write(&notes, prompt(id, name)).ok()?;
            let config = dir.join("opencode.json");
            let body = json!({
                "instructions": [notes],
                "mcp": { "termden": {
                    "type": "local",
                    "command": [ctl, "mcp"],
                    "environment": env,
                    "enabled": true,
                }},
            });
            fs::write(&config, body.to_string()).ok()?;
            Some(format!("OPENCODE_CONFIG={} opencode", q(&config)))
        }
    }
}
