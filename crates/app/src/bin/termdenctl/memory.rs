//! `termdenctl memory <project>`: stdio MCP proxy in front of `rhodon-mcp`.
//! Pins every call to one project folder of the Rhodon agent vault, hides the
//! `project` argument (and so `*`), and adds `compact`.
//! shortcut: agents with a shell can still read the vault on disk; this only
//! scopes the tools.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

const COMPACT_HINT: &str = "Tidy this project's memory: read the notes below that are long, stale or overlapping, merge them into fewer focused notes with `write`, and `delete` what is obsolete. Keep facts the user would be annoyed to repeat; drop anything the code or git history already shows.\n\n";

pub fn serve(project: &str) -> Result<(), String> {
    let mut child = Command::new("rhodon-mcp")
        .env("RHODON_PROJECT", project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("rhodon-mcp: {e}"))?;
    let mut to_child = child.stdin.take().ok_or("no stdin")?;
    let from_child = child.stdout.take().ok_or("no stdout")?;
    let pending: Arc<Mutex<HashMap<String, &'static str>>> = Arc::default();

    let seen = pending.clone();
    let reader = std::thread::spawn(move || {
        let mut out = std::io::stdout();
        for line in BufReader::new(from_child).lines().map_while(Result::ok) {
            let mut msg: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
            let kind = msg
                .get("id")
                .and_then(|id| seen.lock().ok()?.remove(&id.to_string()));
            match kind {
                Some("tools") => {
                    if let Some(tools) = msg["result"]["tools"].as_array_mut() {
                        for tool in tools.iter_mut() {
                            if let Some(props) = tool["inputSchema"]["properties"].as_object_mut() {
                                props.remove("project");
                            }
                        }
                        tools.push(json!({
                            "name": "compact",
                            "description": "Review this project's memory notes and tidy them: returns the note list with sizes and how to merge or delete.",
                            "inputSchema": { "type": "object", "properties": {} },
                        }));
                    }
                }
                Some("compact") => {
                    if let Some(text) = msg["result"]["content"][0]["text"].as_str() {
                        msg["result"]["content"][0]["text"] =
                            format!("{COMPACT_HINT}{text}").into();
                    }
                }
                _ => {}
            }
            let text = if msg.is_null() { line } else { msg.to_string() };
            if writeln!(out, "{text}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    });

    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let mut msg: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let id = msg.get("id").map(|id| id.to_string());
        match (msg["method"].as_str(), id) {
            (Some("tools/list"), Some(id)) => {
                pending
                    .lock()
                    .map_err(|e| e.to_string())?
                    .insert(id, "tools");
            }
            (Some("tools/call"), id) => {
                if msg["params"]["name"] == "compact" {
                    msg["params"]["name"] = "list".into();
                    if let Some(id) = id {
                        pending
                            .lock()
                            .map_err(|e| e.to_string())?
                            .insert(id, "compact");
                    }
                }
                if !msg["params"]["arguments"].is_object() {
                    msg["params"]["arguments"] = json!({});
                }
                msg["params"]["arguments"]["project"] = project.into();
            }
            _ => {}
        }
        let text = if msg.is_null() { line } else { msg.to_string() };
        if writeln!(to_child, "{text}")
            .and_then(|_| to_child.flush())
            .is_err()
        {
            break;
        }
    }
    drop(to_child);
    let _ = reader.join();
    let _ = child.wait();
    Ok(())
}
