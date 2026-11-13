//! `termdenctl mcp`: a stdio MCP server (newline-delimited JSON-RPC) that
//! gives the agent in this tile its termden tools. Agents are launched with
//! it already configured (see the app's launch.rs).
//! shortcut: tools only, no resources/prompts; add them when an agent needs them.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::{mail_read, mail_send, request};

fn tools() -> Value {
    let text = |desc: &str| json!({ "type": "string", "description": desc });
    json!([
        {
            "name": "termden_agents",
            "description": "List every agent (harness) in termden: id, name, tag (role), kind, workspace, status, unread count. `you` is yourself.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "termden_send_mail",
            "description": "Send a message to another agent's mailbox. It is delivered when that agent is idle; replies come back the same way. Prefer this over typing into their terminal.",
            "inputSchema": { "type": "object", "required": ["to", "text"], "properties": {
                "to": text("agent id or name"),
                "text": text("the message; say what you need and how to reply"),
            }},
        },
        {
            "name": "termden_set_name",
            "description": "Give yourself a short unique name (e.g. ada, nova) shown in the sidebar and in mail. Do it once at the start; keep it unless the user asks you to change it.",
            "inputSchema": { "type": "object", "required": ["name"], "properties": {
                "name": text("one short word"),
            }},
        },
        {
            "name": "termden_ask_user_choice",
            "description": "Show the user 2-8 options (e.g. logo or UI design variants) on a local web page and let them pick. Returns the page URL immediately; post it to the user in your reply. The choice arrives later as mail from `user` (call termden_read_mail when told). The user may answer later or skip. Do not wait for it: continue other work or end your turn. Give images as ABSOLUTE paths to png/jpg/svg files.",
            "inputSchema": { "type": "object", "required": ["title", "options"], "properties": {
                "title": text("what is being chosen, e.g. Pick a logo"),
                "question": text("optional extra context"),
                "options": { "type": "array", "minItems": 2, "maxItems": 8, "items": {
                    "type": "object", "required": ["label"], "properties": {
                        "label": text("short name"),
                        "description": text("one line on what makes it different"),
                        "image": text("absolute path to a preview image"),
                    }}},
            }},
        },
        {
            "name": "termden_set_title",
            "description": "Set a short title (max 60 chars) saying what you are working on, shown under your name in the sidebar. Set it after your first task and update it when your focus changes.",
            "inputSchema": { "type": "object", "required": ["title"], "properties": {
                "title": text("e.g. Fixing sidebar alignment"),
            }},
        },
        {
            "name": "termden_set_tag",
            "description": "Set your own short role tag (e.g. frontend, backend, debugger) so other agents know what you do. Shown in the sidebar and in termden_agents and mail.",
            "inputSchema": { "type": "object", "required": ["tag"], "properties": {
                "tag": text("one short word, max 12 chars; empty clears"),
            }},
        },
        {
            "name": "termden_set_pronouns",
            "description": "Choose your own pronouns (e.g. she/her, he/him, they/them, it/its) so the user and other agents can refer to you correctly. Shown next to your name.",
            "inputSchema": { "type": "object", "required": ["pronouns"], "properties": {
                "pronouns": text("e.g. they/them, max 16 chars; empty clears"),
            }},
        },
        {
            "name": "termden_read_mail",
            "description": "Read and clear your unread messages.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "termden_read_screen",
            "description": "Read the recent terminal output of another agent (read-only).",
            "inputSchema": { "type": "object", "required": ["agent"], "properties": {
                "agent": text("agent id or name"),
                "lines": { "type": "integer", "description": "how many trailing lines (default 80)" },
            }},
        },
    ])
}

fn call(name: &str, args: &Value) -> Result<String, String> {
    let me = std::env::var("TERMDEN_HARNESS").unwrap_or_else(|_| "0".into());
    let arg = |key: &str| {
        args[key]
            .as_str()
            .map(str::to_string)
            .ok_or(format!("missing `{key}`"))
    };
    match name {
        "termden_agents" => {
            let state: Value =
                serde_json::from_str(&request("state")?).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for w in state["workspaces"].as_array().into_iter().flatten() {
                for h in w["harnesses"].as_array().into_iter().flatten() {
                    out.push(json!({
                        "id": h["id"], "name": h["name"], "kind": h["kind"],
                        "workspace": w["name"], "status": h["status"],
                        "tag": h["tag"], "pronouns": h["pronouns"], "title": h["agent_title"], "unread": h["unread"], "you": h["id"].to_string() == me,
                    }));
                }
            }
            Ok(json!(out).to_string())
        }
        "termden_send_mail" => {
            mail_send(&me, &arg("to")?, &arg("text")?)?;
            Ok("delivered".into())
        }
        "termden_set_name" => {
            request(&format!("rename {me} {}", arg("name")?.replace('\n', " ")))?;
            Ok("name set".into())
        }
        "termden_ask_user_choice" => {
            let body = json!({
                "from": me, "title": args["title"], "question": args["question"],
                "options": args["options"],
            });
            let url = request(&format!("choice {body}"))?;
            Ok(format!(
                "Preview started at {url}\nTell the user in your reply, with that exact link on its own line, that the options are ready to pick there (they Ctrl+click it). Their pick will arrive as mail from `user`; do not wait for it."
            ))
        }
        "termden_set_title" => {
            request(&format!("title {me} {}", arg("title")?.replace('\n', " ")))?;
            Ok("title set".into())
        }
        "termden_set_tag" => {
            request(&format!("tag {me} {}", arg("tag")?.replace('\n', " ")))?;
            Ok("tag set".into())
        }
        "termden_set_pronouns" => {
            request(&format!(
                "pronouns {me} {}",
                arg("pronouns")?.replace('\n', " ")
            ))?;
            Ok("pronouns set".into())
        }
        "termden_read_mail" => {
            let mail = mail_read(&me)?;
            if mail.is_empty() {
                return Ok("no new messages".into());
            }
            Ok(mail
                .iter()
                .map(|m| {
                    format!(
                        "From {}:\n{}",
                        m["from"].as_str().unwrap_or("?"),
                        m["text"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n---\n\n"))
        }
        "termden_read_screen" => {
            let lines = args["lines"].as_u64().unwrap_or(80).min(2000);
            request(&format!("history {} {lines}", arg("agent")?))
        }
        other => Err(format!("unknown tool {other}")),
    }
}

fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let result = match msg["method"].as_str()? {
        "initialize" => json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2024-11-05"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "termden", "version": env!("CARGO_PKG_VERSION") },
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let params = &msg["params"];
            let (text, is_error) =
                match call(params["name"].as_str().unwrap_or(""), &params["arguments"]) {
                    Ok(text) => (text, false),
                    Err(error) => (error, true),
                };
            json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
        }
        other => {
            return Some(json!({ "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": format!("unknown method {other}") } }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

pub fn serve() -> Result<(), String> {
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(reply) = handle(&msg) {
            writeln!(out, "{reply}").map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_and_list_tools() {
        let init = handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}})).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        let list = handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 9);
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    }
}
