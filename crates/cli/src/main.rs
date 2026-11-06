//! `termdenctl`: talk to the running termden over its control socket.
//!
//! Built for agents developing termden from inside termden: inspect state,
//! read another tile's screen, start harnesses, reload, take screenshots.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use termden_core::state::control_socket;

mod mcp;

const USAGE: &str = "\
usage: termdenctl <command>

Harnesses are named by id or name; `self` is the harness this runs in.

  ping | state | screen [h] | history <h> [lines]
  send [--enter] <h> <text>   type into a harness (--enter submits it)
  key <h> <key>...            enter, escape, ctrl-c, up, tab, shift-tab, ...
  wait <h> [secs]             block until the harness stops producing output
                              (do this after `add`, before the first `send`)
  focus <h> | close <h> | restart <h> | rename <h> <name> | tag <h> [text] | title <h> [text]
  pronouns <h> [text]         set a harness's pronouns (empty clears)
  add <claude|opencode|pi|shell> [ws] | add-workspace <dir>
  activate <ws> | rename-workspace <ws> <name>
  mail send <h> <text>        message another agent (delivered when it is idle)
  mail read [h]               read and clear unread messages (default: self)
  mcp                         stdio MCP server with these tools (agents get it
                              pre-configured; you should not need to run it)
  log <text>                  post to the activity feed (as `self` if set)
  settings [close] | theme | reload
  screenshot [out.png] [--live]
      default: render a read-only mirror of the running app headlessly
               (never touches your desktop; same tmux sessions, same size)
      --live:  grab the real window with grim (must be visible on screen)";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let me = std::env::var("TERMDEN_HARNESS").ok();
    for arg in args.iter_mut() {
        if arg == "self" {
            match &me {
                Some(id) => *arg = id.clone(),
                None => {
                    eprintln!("termdenctl: `self` needs TERMDEN_HARNESS (run inside a tile)");
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    let result = match args.first().map(String::as_str) {
        None | Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("screenshot") => screenshot(&args[1..]),
        Some("wait") => wait(&args[1..]),
        Some("log") => {
            let who = me.unwrap_or_else(|| "0".into());
            request(&format!("log {who} {}", args[1..].join(" "))).map(drop)
        }
        Some("send") => send(&args[1..]),
        Some("mcp") => mcp::serve(),
        Some("mail") => mail(&args[1..], me.as_deref()),
        Some(_) => request(&args.join(" ")).map(|reply| println!("{reply}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("termdenctl: {error}");
            ExitCode::FAILURE
        }
    }
}

fn send(args: &[String]) -> Result<(), String> {
    let enter = args.iter().any(|a| a == "--enter");
    let args: Vec<&String> = args.iter().filter(|a| *a != "--enter").collect();
    let [harness, text @ ..] = args.as_slice() else {
        return Err("usage: send [--enter] <harness> <text>".into());
    };
    let text: Vec<&str> = text.iter().map(|t| t.as_str()).collect();
    // The protocol is one line per request; multi-line text goes line by line.
    for (index, line) in text.join(" ").split('\n').enumerate() {
        if index > 0 {
            request(&format!("key {harness} shift-enter"))?;
        }
        if !line.is_empty() {
            request(&format!("send {harness} {line}"))?;
        }
    }
    if enter {
        // Let TUIs treat the text as typed, not as part of a paste.
        std::thread::sleep(std::time::Duration::from_millis(150));
        request(&format!("key {harness} enter"))?;
    }
    Ok(())
}

/// Poll `state` until the harness has been quiet (not `working`) for 2s.
fn wait(args: &[String]) -> Result<(), String> {
    let harness = args.first().ok_or("usage: wait <harness> [secs]")?;
    let limit: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(600.0);
    let start = std::time::Instant::now();
    let mut quiet_since = None;
    loop {
        let state: serde_json::Value =
            serde_json::from_str(&request("state")?).map_err(|e| e.to_string())?;
        let found = state["workspaces"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|w| w["harnesses"].as_array().into_iter().flatten())
            .find(|h| h["id"].to_string() == *harness || h["name"] == harness.as_str())
            .cloned()
            .ok_or("unknown harness")?;
        let busy = found["status"] == "working" && found["live"] == true;
        match (busy, quiet_since) {
            (true, _) => quiet_since = None,
            (false, None) => quiet_since = Some(std::time::Instant::now()),
            (false, Some(at)) if at.elapsed().as_secs_f64() >= 2.0 => {
                println!("{}", found["status"].as_str().unwrap_or("idle"));
                return Ok(());
            }
            _ => {}
        }
        if start.elapsed().as_secs_f64() > limit {
            return Err("timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn mail(args: &[String], me: Option<&str>) -> Result<(), String> {
    let from = me.unwrap_or("0");
    match args {
        [cmd, to, text @ ..] if cmd == "send" && !text.is_empty() => {
            mail_send(from, to, &text.join(" "))?;
            println!("delivered");
        }
        [cmd, rest @ ..] if cmd == "read" => {
            let who = rest
                .first()
                .map(String::as_str)
                .or(me)
                .ok_or("usage: mail read <h>")?;
            let mail = mail_read(who)?;
            if mail.is_empty() {
                println!("no new messages");
            }
            for m in mail {
                println!(
                    "From {}:\n{}\n",
                    m["from"].as_str().unwrap_or("?"),
                    m["text"].as_str().unwrap_or("")
                );
            }
        }
        _ => return Err("usage: mail send <h> <text> | mail read [h]".into()),
    }
    Ok(())
}

fn mail_send(from: &str, to: &str, text: &str) -> Result<(), String> {
    let body = serde_json::json!({ "from": from, "to": to, "text": text });
    request(&format!("mail-send {body}")).map(drop)
}

fn mail_read(who: &str) -> Result<Vec<serde_json::Value>, String> {
    serde_json::from_str(&request(&format!("mail-read {who}"))?).map_err(|e| e.to_string())
}

fn request(line: &str) -> Result<String, String> {
    let path = control_socket();
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("termden is not running ({}: {e})", path.display()))?;
    writeln!(stream, "{line}").map_err(|e| e.to_string())?;
    let mut reply = String::new();
    stream
        .read_to_string(&mut reply)
        .map_err(|e| e.to_string())?;
    let reply = reply.trim_end().to_string();
    match reply.strip_prefix("error: ") {
        Some(message) => Err(message.to_string()),
        None => Ok(reply),
    }
}

fn screenshot(args: &[String]) -> Result<(), String> {
    let live = args.iter().any(|a| a == "--live");
    let out = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("termden-shot.png"));
    let out = std::path::absolute(&out).map_err(|e| e.to_string())?;

    if live {
        return live_screenshot(&out);
    }

    let state: serde_json::Value =
        serde_json::from_str(&request("state")?).map_err(|e| e.to_string())?;
    let size = |key: &str| state["window"][key].as_f64().unwrap_or(0.0).round() as u32;
    let (width, height) = (size("width"), size("height"));
    if width == 0 || height == 0 {
        return Err("app has not rendered yet".into());
    }

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/headless-shot.sh");
    let status = Command::new(script)
        .arg(&out)
        .arg(width.to_string())
        .arg(height.to_string())
        .env("TERMDEN_SHOT_MODE", "mirror")
        // A rebuild would hot-reload the live app under our feet.
        .env("TERMDEN_SHOT_NOBUILD", "1")
        .status()
        .map_err(|e| format!("could not run headless-shot.sh: {e}"))?;
    if !status.success() {
        return Err("headless capture failed".into());
    }
    Ok(())
}

/// Grab the real window through Hyprland + grim.
fn live_screenshot(out: &PathBuf) -> Result<(), String> {
    let clients = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .map_err(|e| format!("hyprctl: {e}"))?;
    let clients: serde_json::Value =
        serde_json::from_slice(&clients.stdout).map_err(|e| e.to_string())?;
    let window = clients
        .as_array()
        .and_then(|all| all.iter().find(|c| c["class"] == "termden"))
        .ok_or("no termden window found")?;
    let num = |v: &serde_json::Value| v.as_i64().unwrap_or(0);
    let geometry = format!(
        "{},{} {}x{}",
        num(&window["at"][0]),
        num(&window["at"][1]),
        num(&window["size"][0]),
        num(&window["size"][1])
    );
    let status = Command::new("grim")
        .args(["-g", &geometry])
        .arg(out)
        .status()
        .map_err(|e| format!("grim: {e}"))?;
    if !status.success() {
        return Err("grim failed".into());
    }
    println!("{}", out.display());
    Ok(())
}
