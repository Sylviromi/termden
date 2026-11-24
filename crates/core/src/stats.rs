//! Per-harness usage, read from the agents' own session data.
//!
//! termden does not count tokens itself and persists nothing: Claude Code's
//! transcript JSONL and opencode's SQLite database are the source of truth, so
//! the numbers match what the agents report and survive app restarts.
//!
//! Every read here is read-only and best effort. A missing or half-written
//! file means "unknown" (`None`), never zero.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::state::runtime_dir;
use crate::{HarnessId, HarnessKind};

/// Token counts as the providers report them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    /// Prompt tokens the model processed from scratch. Claude reports these
    /// split into uncached input and cache writes; both are new input.
    pub fn fresh_input(self) -> u64 {
        self.input + self.cache_write
    }

    /// Everything that passed through the model, cache included.
    pub fn total(self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, other: Usage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }

    fn sub(&mut self, other: Usage) {
        self.input -= other.input.min(self.input);
        self.output -= other.output.min(self.output);
        self.cache_read -= other.cache_read.min(self.cache_read);
        self.cache_write -= other.cache_write.min(self.cache_write);
    }
}

/// Every Claude Code session a harness has been in, oldest first: its
/// SessionStart hook (`termdenctl session-hook`) adds one line on start,
/// `/clear` and `--resume`.
///
/// A harness launched without that hook has none until it is restarted.
pub fn claude_sessions(id: HarnessId) -> Vec<String> {
    fs::read_to_string(harness_dir(id).join("session.id"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// One plan limit window (a 5-hour session, a week, a month).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limit {
    pub label: &'static str,
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: Option<u64>,
}

/// What the status line input of one Claude tile says.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeStatus {
    /// Unix seconds the status line last ran.
    pub at: u64,
    pub context_percent: Option<f64>,
    /// Plan limits; empty for API-key sessions.
    pub limits: Vec<Limit>,
}

/// The status line input `termdenctl statusline` saved for a harness.
pub fn claude_status(id: HarnessId) -> Option<ClaudeStatus> {
    let path = harness_dir(id).join("status.json");
    let at = fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();
    parse_status(&fs::read_to_string(path).ok()?, at)
}

fn parse_status(text: &str, at: u64) -> Option<ClaudeStatus> {
    let value: Value = serde_json::from_str(text).ok()?;
    let limits = [("5h session", "five_hour"), ("week", "seven_day")]
        .into_iter()
        .filter_map(|(label, key)| {
            let limit = &value["rate_limits"][key];
            Some(Limit {
                label,
                used_percent: limit["used_percentage"].as_f64()?,
                resets_at: limit["resets_at"].as_u64(),
            })
        })
        .collect();
    Some(ClaudeStatus {
        at,
        context_percent: value["context_window"]["used_percentage"].as_f64(),
        limits,
    })
}

/// opencode Go plan limits, asked of opencode itself (it holds the key):
/// `opencode api --standalone GET /api/experimental/usage/limits`. Takes a
/// couple of seconds; `None` when opencode is missing or the account has no Go
/// plan.
pub fn opencode_go_limits() -> Option<Vec<Limit>> {
    let out = std::process::Command::new("timeout")
        .args(["30", "opencode", "api", "--standalone", "GET"])
        .arg("/api/experimental/usage/limits")
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status.success().then_some(())?;
    parse_go_limits(&String::from_utf8_lossy(&out.stdout))
}

fn parse_go_limits(text: &str) -> Option<Vec<Limit>> {
    let value: Value = serde_json::from_str(text).ok()?;
    let go = value["data"]["go"].as_object()?;
    let limits: Vec<Limit> = [
        ("5h session", "rolling"),
        ("week", "weekly"),
        ("month", "monthly"),
    ]
    .into_iter()
    .filter_map(|(label, key)| {
        let limit = go.get(key)?;
        Some(Limit {
            label,
            used_percent: limit["percent"].as_f64()?,
            resets_at: limit["resetsAt"].as_u64().map(|ms| ms / 1000),
        })
    })
    .collect();
    (!limits.is_empty()).then_some(limits)
}

pub fn harness_dir(id: HarnessId) -> PathBuf {
    runtime_dir().join("harness").join(id.raw().to_string())
}

fn claude_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR")
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").ok()?;
    Some(Path::new(&home).join(".claude"))
}

fn opencode_db() -> Option<PathBuf> {
    let data = match std::env::var("XDG_DATA_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var("HOME").ok()?).join(".local/share"),
    };
    let path = data.join("opencode").join("opencode.db");
    path.is_file().then_some(path)
}

fn opencode_config_dir() -> Option<PathBuf> {
    let config = match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var("HOME").ok()?).join(".config"),
    };
    Some(config.join("opencode"))
}

/// Incremental reader for one Claude Code session: its transcript plus the
/// subagent transcripts Claude Code keeps in `<session>/subagents/`.
///
/// Transcripts grow to tens of megabytes, so each file is only read from its
/// last offset and usage is kept per assistant message id: Claude Code repeats
/// the same message as it streams and for each content block (and a forked
/// subagent starts with a copy of its parent's last message), and only the
/// last copy carries the final count.
pub struct ClaudeLog {
    session: String,
    path: Option<PathBuf>,
    offsets: HashMap<PathBuf, u64>,
    seen: HashMap<String, Usage>,
    usage: Usage,
}

impl ClaudeLog {
    pub fn new(session: &str) -> Self {
        Self {
            session: session.to_string(),
            path: None,
            offsets: HashMap::new(),
            seen: HashMap::new(),
            usage: Usage::default(),
        }
    }

    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// Whether the transcript was found; before the first message it is not
    /// written yet and the agent's usage is simply unknown.
    pub fn found(&self) -> bool {
        self.path.is_some()
    }

    /// Where the transcript lives: `~/.claude/projects/<encoded cwd>/<id>.jsonl`
    /// but the encoding is not ours to guess, so look for the id.
    fn locate(&mut self) -> Option<PathBuf> {
        if let Some(path) = &self.path
            && path.exists()
        {
            return Some(path.clone());
        }
        let projects = claude_dir()?.join("projects");
        let file = format!("{}.jsonl", self.session);
        self.path = fs::read_dir(&projects)
            .ok()?
            .flatten()
            .map(|entry| entry.path().join(&file))
            .find(|candidate| candidate.is_file());
        self.path.clone()
    }

    /// Read whatever was appended since last time. Returns whether the numbers
    /// moved.
    pub fn refresh(&mut self) -> bool {
        let Some(main) = self.locate() else {
            return false;
        };
        let mut files = vec![main.clone()];
        if let Ok(entries) = fs::read_dir(main.with_extension("").join("subagents")) {
            files.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl")),
            );
        }
        // A file replaced under us (shorter than what we read) would leave its
        // old messages counted; start over rather than read junk.
        let shrunk = files.iter().any(|file| {
            let offset = self.offsets.get(file).copied().unwrap_or(0);
            fs::metadata(file).is_ok_and(|meta| meta.len() < offset)
        });
        if shrunk {
            self.offsets.clear();
            self.seen.clear();
            self.usage = Usage::default();
        }
        let mut changed = shrunk;
        for file in files {
            changed |= self.read(&file);
        }
        changed
    }

    fn read(&mut self, path: &Path) -> bool {
        let offset = self.offsets.get(path).copied().unwrap_or(0);
        if fs::metadata(path).map_or(true, |meta| meta.len() == offset) {
            return false;
        }
        let Ok(mut file) = File::open(path) else {
            return false;
        };
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return false;
        }
        let mut reader = BufReader::with_capacity(64 * 1024, file);
        let mut changed = false;
        let mut read_to = offset;
        let mut line = String::new();
        loop {
            line.clear();
            let Ok(read) = reader.read_line(&mut line) else {
                break;
            };
            // A write may be caught mid-line; leave it for the next refresh.
            if read == 0 || !line.ends_with('\n') {
                break;
            }
            read_to += read as u64;
            changed |= self.parse(&line);
        }
        self.offsets.insert(path.to_path_buf(), read_to);
        changed
    }

    fn parse(&mut self, line: &str) -> bool {
        if !line.contains("\"usage\"") {
            return false;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let Some(message) = value.get("message") else {
            return false;
        };
        let Some(usage) = message.get("usage") else {
            return false;
        };
        // Without an id the copy cannot be deduplicated; leave it out rather
        // than inflate the total.
        let Some(id) = message.get("id").and_then(Value::as_str) else {
            return false;
        };
        let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
        let usage = Usage {
            input: count("input_tokens"),
            output: count("output_tokens"),
            cache_read: count("cache_read_input_tokens"),
            cache_write: count("cache_creation_input_tokens"),
        };
        if let Some(previous) = self.seen.insert(id.to_string(), usage) {
            self.usage.sub(previous);
        }
        self.usage.add(usage);
        true
    }
}

/// Tokens an opencode session (and its child sessions) used, from its database.
///
/// Newer opencode builds write `session_v2`, older ones `session`; both carry
/// the same token columns. Unknown schemas mean `None`, not zero.
pub fn opencode_usage(session: &str) -> Option<Usage> {
    use rusqlite::{Connection, OpenFlags};

    let db = opencode_db()?;
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(db, flags).ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(400));
    for table in ["session_v2", "session"] {
        // A missing session still returns one row of zeros, so count as well.
        let sql = format!(
            "SELECT COUNT(*), COALESCE(SUM(tokens_input),0), COALESCE(SUM(tokens_output),0), \
             COALESCE(SUM(tokens_cache_read),0), COALESCE(SUM(tokens_cache_write),0) \
             FROM {table} WHERE id = ?1 OR parent_id = ?1"
        );
        let row = conn.query_row(&sql, [session], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Usage {
                    input: row.get::<_, i64>(1)? as u64,
                    output: row.get::<_, i64>(2)? as u64,
                    cache_read: row.get::<_, i64>(3)? as u64,
                    cache_write: row.get::<_, i64>(4)? as u64,
                },
            ))
        });
        if let Ok((count, usage)) = row
            && count > 0
        {
            return Some(usage);
        }
    }
    None
}

/// Prompt-cache TTL assumed for opencode: the Go gateway keeps a prefix about
/// five minutes (no published figure; upstream providers range from ~5 to 35
/// minutes), so past this a reply re-reads the whole context uncached.
pub const OPENCODE_CACHE_TTL_MS: u64 = 5 * 60 * 1000;

/// Context size (tokens) of an opencode session's latest finished reply and
/// when it finished (unix ms), for the cold-cache warning.
pub fn opencode_last_reply(session: &str) -> Option<(u64, u64)> {
    use rusqlite::{Connection, OpenFlags};

    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(opencode_db()?, flags).ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(400));
    conn.query_row(
        "SELECT json_extract(data,'$.time.completed'), \
                COALESCE(json_extract(data,'$.tokens.input'),0) \
                + COALESCE(json_extract(data,'$.tokens.cache.read'),0) \
                + COALESCE(json_extract(data,'$.tokens.cache.write'),0) \
         FROM session_message \
         WHERE session_id = ?1 AND type = 'assistant' \
           AND json_extract(data,'$.time.completed') IS NOT NULL \
         ORDER BY seq DESC LIMIT 1",
        [session],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
    )
    .ok()
}

/// The opencode sessions a termden tile has used since `since_ms`, found
/// through the per-harness instructions file opencode records with every
/// session (`/new` adds one).
///
/// `--session <unknown-id>` is not an option: the TUI ignores it and resumes
/// the previous session instead. The instructions path is unique to the
/// harness.
pub fn opencode_sessions(id: HarnessId, since_ms: u64) -> Vec<String> {
    use rusqlite::{Connection, OpenFlags};

    let mut sessions = Vec::new();
    let Some(db) = opencode_db() else {
        return sessions;
    };
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(conn) = Connection::open_with_flags(db, flags) else {
        return sessions;
    };
    let _ = conn.busy_timeout(Duration::from_millis(400));
    let needle = format!("termden/harness/{}/instructions.md", id.raw());
    let Ok(mut stmt) = conn.prepare(
        "SELECT s.id, i.initial_values FROM session_v2 s \
             JOIN instruction_state i ON i.session_id = s.id \
             WHERE s.parent_id IS NULL AND s.time_created >= ?1 \
             ORDER BY s.time_created",
    ) else {
        return sessions;
    };
    let Ok(mut rows) = stmt.query([since_ms as i64]) else {
        return sessions;
    };
    loop {
        let row = match rows.next() {
            Ok(Some(row)) => row,
            _ => return sessions,
        };
        let Ok(session) = row.get::<_, String>(0) else {
            continue;
        };
        let Ok(initial) = row.get::<_, String>(1) else {
            continue;
        };
        let Ok(values) = serde_json::from_str::<Value>(&initial) else {
            continue;
        };
        let Some(hash) = values.get("core/instructions").and_then(Value::as_str) else {
            continue;
        };
        let blob: Option<String> = conn
            .query_row(
                "SELECT value FROM instruction_blob WHERE hash = ?1",
                [hash],
                |row| row.get(0),
            )
            .ok();
        if blob.is_some_and(|blob| blob.contains(&needle)) {
            sessions.push(session);
        }
    }
}

/// MCP servers that will actually be on for a harness started in `cwd`.
///
/// Read from the config termden writes for the harness, the project files the
/// agent loads itself (`.mcp.json`, `opencode.json`), the user's config
/// (`~/.claude.json` user and project scope, enabled Claude plugins, opencode's
/// global config). Not listed: claude.ai connectors and servers only an
/// agent's CLI knows about.
pub fn mcp_servers(cwd: &Path, id: HarnessId, kind: HarnessKind) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    match kind {
        HarnessKind::ClaudeCode => {
            push_keys(&mut names, &harness_dir(id).join("mcp.json"), "mcpServers");
            push_keys(&mut names, &cwd.join(".mcp.json"), "mcpServers");
            for name in claude_json_servers(cwd)
                .into_iter()
                .chain(claude_plugin_servers())
            {
                push_name(&mut names, name);
            }
        }
        HarnessKind::OpenCode => {
            push_mcp_block(&mut names, &harness_dir(id).join("opencode.json"));
            for name in ["opencode.json", "opencode.jsonc"] {
                push_mcp_block(&mut names, &cwd.join(name));
                push_mcp_block(&mut names, &cwd.join(".opencode").join(name));
            }
            if let Some(dir) = opencode_config_dir() {
                for name in ["opencode.json", "opencode.jsonc"] {
                    push_mcp_block(&mut names, &dir.join(name));
                }
            }
        }
        HarnessKind::Pi | HarnessKind::Shell => {}
    }
    names.sort();
    names
}

fn push_name(names: &mut Vec<String>, name: String) {
    if !names.iter().any(|existing| existing == &name) {
        names.push(name);
    }
}

fn push_keys(names: &mut Vec<String>, path: &Path, key: &str) {
    let Some(value) = load_json(path) else {
        return;
    };
    let Some(servers) = value.get(key).and_then(Value::as_object) else {
        return;
    };
    for name in servers.keys() {
        push_name(names, name.clone());
    }
}

/// `{"mcp": {"name": {"enabled": false, ...}}}`; disabled servers are off.
/// Some opencode configs wrap the servers one level deeper
/// (`{"mcp": {"servers": {"name": ...}}}`), so unwrap that too.
fn push_mcp_block(names: &mut Vec<String>, path: &Path) {
    let Some(value) = load_json(path) else {
        return;
    };
    let Some(servers) = value.get("mcp").and_then(Value::as_object) else {
        return;
    };
    for (name, config) in servers {
        let server = config.get("type").is_some()
            || config.get("command").is_some()
            || config.get("url").is_some();
        if server {
            if enabled(config) {
                push_name(names, name.clone());
            }
        } else if let Some(inner) = config.as_object() {
            for (name, config) in inner {
                if config.is_object() && enabled(config) {
                    push_name(names, name.clone());
                }
            }
        }
    }
}

fn enabled(config: &Value) -> bool {
    !config
        .get("enabled")
        .and_then(Value::as_bool)
        .is_some_and(|enabled| !enabled)
}

/// Config files may be `.jsonc` (opencode documents that flavour): comments
/// and trailing commas are legal there, not in `serde_json`.
fn load_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text)
        .ok()
        .or_else(|| serde_json::from_str(&strip_jsonc(&text)).ok())
}

fn strip_jsonc(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut string = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if string {
            out.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            i += 1;
            continue;
        }
        if byte == b'"' {
            string = true;
            out.push(byte);
            i += 1;
        } else if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            out.push(byte);
            i += 1;
        }
    }
    // Drop commas that are followed only by whitespace and a closing bracket.
    let mut cleaned: Vec<u8> = Vec::with_capacity(out.len());
    let mut string = false;
    let mut escaped = false;
    for (index, &byte) in out.iter().enumerate() {
        if string {
            cleaned.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        if byte == b'"' {
            string = true;
            cleaned.push(byte);
            continue;
        }
        if byte == b',' {
            let next = out[index + 1..]
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .copied();
            if matches!(next, Some(b'}') | Some(b']')) {
                continue;
            }
        }
        cleaned.push(byte);
    }
    String::from_utf8_lossy(&cleaned).into_owned()
}

/// MCP servers from `~/.claude.json`: user scope plus the project scope of
/// `cwd`. That file is large and rewritten often, so keep the parsed names
/// until its mtime moves.
fn claude_json_servers(cwd: &Path) -> Vec<String> {
    type Parsed = (Vec<String>, HashMap<String, Vec<String>>);
    static CACHE: Mutex<Option<(SystemTime, Parsed)>> = Mutex::new(None);
    let Some(path) = std::env::var("HOME")
        .ok()
        .map(|home| Path::new(&home).join(".claude.json"))
    else {
        return Vec::new();
    };
    let Ok(modified) = fs::metadata(&path).and_then(|meta| meta.modified()) else {
        return Vec::new();
    };
    let pick = |(user, projects): &Parsed| {
        let mut names = user.clone();
        names.extend(
            projects
                .get(&*cwd.to_string_lossy())
                .into_iter()
                .flatten()
                .cloned(),
        );
        names
    };
    let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
    if let Some((stamp, parsed)) = cache.as_ref()
        && *stamp == modified
    {
        return pick(parsed);
    }
    let keys = |value: &Value| -> Vec<String> {
        value
            .get("mcpServers")
            .and_then(Value::as_object)
            .map(|servers| servers.keys().cloned().collect())
            .unwrap_or_default()
    };
    let value = load_json(&path).unwrap_or(Value::Null);
    let projects = value
        .get("projects")
        .and_then(Value::as_object)
        .map(|projects| {
            projects
                .iter()
                .map(|(dir, project)| (dir.clone(), keys(project)))
                .filter(|(_, names)| !names.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let parsed = (keys(&value), projects);
    let names = pick(&parsed);
    *cache = Some((modified, parsed));
    names
}

/// MCP servers bundled with the Claude Code plugins enabled in
/// `~/.claude/settings.json`: a plugin's `.mcp.json`, or `mcpServers` in its
/// manifest (inline or a path).
fn claude_plugin_servers() -> Vec<String> {
    let mut names = Vec::new();
    let Some(dir) = claude_dir() else {
        return names;
    };
    let Some(settings) = load_json(&dir.join("settings.json")) else {
        return names;
    };
    let Some(installed) = load_json(&dir.join("plugins").join("installed_plugins.json")) else {
        return names;
    };
    let enabled = settings
        .get("enabledPlugins")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(_, on)| on.as_bool() == Some(true));
    for (plugin, _) in enabled {
        let Some(root) = installed["plugins"][plugin][0]["installPath"].as_str() else {
            continue;
        };
        let root = Path::new(root);
        let manifest = load_json(&root.join(".claude-plugin").join("plugin.json"));
        let servers = match manifest.as_ref().map(|manifest| &manifest["mcpServers"]) {
            Some(Value::Object(inline)) => Some(Value::Object(inline.clone())),
            Some(Value::String(file)) => load_json(&root.join(file)),
            _ => load_json(&root.join(".mcp.json")),
        };
        // `.mcp.json` files come with and without the `mcpServers` wrapper.
        let servers = servers.map(|value| match value.get("mcpServers") {
            Some(inner) => inner.clone(),
            None => value,
        });
        for name in servers
            .as_ref()
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(name, _)| name)
        {
            push_name(&mut names, name.clone());
        }
    }
    names
}

/// Compact token count for the sidebar: `842`, `9.8k`, `98k`, `1.28M`.
/// Trailing zeros are dropped (`1M`, not `1.00M`).
pub fn short_tokens(tokens: u64) -> String {
    match tokens {
        0..=999 => tokens.to_string(),
        1_000..=9_999 => format!("{}k", trim_zeros(format!("{:.1}", tokens as f64 / 1_000.))),
        10_000..=999_999 => format!("{:.0}k", tokens as f64 / 1_000.),
        1_000_000..=9_999_999 => format!(
            "{}M",
            trim_zeros(format!("{:.2}", tokens as f64 / 1_000_000.))
        ),
        _ => format!("{:.0}M", tokens as f64 / 1_000_000.),
    }
}

fn trim_zeros(mut text: String) -> String {
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

/// How long ago something started: `<1m`, `41m`, `2h 14m`, `3d 4h`.
pub fn short_age(seconds: u64) -> String {
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    match seconds {
        0..=59 => "<1m".into(),
        60..=3599 => format!("{minutes}m"),
        3600..=86_399 => match minutes % 60 {
            0 => format!("{hours}h"),
            rest => format!("{hours}h {rest:02}m"),
        },
        _ => format!("{days}d {}h", hours % 24),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(id: &str, input: u64, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"id":"{id}","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_read_input_tokens":10,"cache_creation_input_tokens":5}}}}}}"#
        )
    }

    #[test]
    fn repeated_message_ids_are_counted_once_with_their_last_numbers() {
        let mut log = ClaudeLog::new("session");
        assert!(log.parse(&assistant("a", 100, 10)));
        // The same message is written again as it streams; keep the newer one.
        assert!(log.parse(&assistant("a", 140, 12)));
        assert!(log.parse(&assistant("b", 7, 3)));
        assert_eq!(
            log.usage(),
            Usage {
                input: 147,
                output: 15,
                cache_read: 20,
                cache_write: 10,
            }
        );
    }

    #[test]
    fn subagent_transcripts_count_once_and_appends_are_picked_up() {
        let dir = std::env::temp_dir().join(format!("termden-claude-{}", std::process::id()));
        let subagents = dir.join("s").join("subagents");
        fs::create_dir_all(&subagents).unwrap();
        let main = dir.join("s.jsonl");
        fs::write(&main, assistant("parent", 100, 10) + "\n").unwrap();
        // A forked subagent starts with a copy of the parent's message.
        let sub = [assistant("parent", 100, 10), assistant("child", 50, 5)].join("\n");
        fs::write(subagents.join("agent-a.jsonl"), sub + "\n").unwrap();
        let mut log = ClaudeLog::new("s");
        log.path = Some(main.clone());
        assert!(log.refresh());
        assert_eq!((log.usage().input, log.usage().output), (150, 15));
        assert!(!log.refresh());
        let mut file = fs::OpenOptions::new().append(true).open(&main).unwrap();
        std::io::Write::write_all(&mut file, (assistant("next", 1, 1) + "\n").as_bytes()).unwrap();
        assert!(log.refresh());
        assert_eq!(log.usage().input, 151);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lines_without_a_message_id_are_ignored() {
        let mut log = ClaudeLog::new("session");
        let line =
            r#"{"type":"assistant","message":{"usage":{"input_tokens":5,"output_tokens":5}}}"#;
        assert!(!log.parse(line));
        assert_eq!(log.usage(), Usage::default());
    }

    #[test]
    fn status_line_input_gives_limits_and_context() {
        let text = r#"{"context_window":{"used_percentage":4},"rate_limits":{"five_hour":{"used_percentage":68,"resets_at":1795013400},"seven_day":{"used_percentage":29.5}}}"#;
        let status = parse_status(text, 7).unwrap();
        assert_eq!(status.context_percent, Some(4.));
        assert_eq!(
            status.limits,
            [
                Limit {
                    label: "5h session",
                    used_percent: 68.,
                    resets_at: Some(1_795_013_400)
                },
                Limit {
                    label: "week",
                    used_percent: 29.5,
                    resets_at: None
                },
            ]
        );
        assert!(parse_status("{}", 7).unwrap().limits.is_empty());
    }

    #[test]
    fn opencode_go_limits_come_in_seconds() {
        let text = r#"{"data":{"go":{"rolling":{"percent":0,"limited":false,"resetsAt":1795024623239},"weekly":{"percent":8,"resetsAt":1795392000000},"monthly":{"percent":4,"resetsAt":1796897924000}},"budgets":[]}}"#;
        let limits = parse_go_limits(text).unwrap();
        assert_eq!(limits.len(), 3);
        assert_eq!(limits[0].resets_at, Some(1_795_024_623));
        assert_eq!((limits[2].label, limits[2].used_percent), ("month", 4.));
        assert_eq!(parse_go_limits(r#"{"data":{"go":null}}"#), None);
    }

    #[test]
    fn token_counts_stay_readable() {
        assert_eq!(short_tokens(0), "0");
        assert_eq!(short_tokens(842), "842");
        assert_eq!(short_tokens(1_000), "1k");
        assert_eq!(short_tokens(9_800), "9.8k");
        assert_eq!(short_tokens(98_200), "98k");
        assert_eq!(short_tokens(1_000_000), "1M");
        assert_eq!(short_tokens(1_280_000), "1.28M");
        assert_eq!(short_tokens(9_800_000), "9.8M");
        assert_eq!(short_tokens(83_000_000), "83M");
    }

    #[test]
    fn ages_use_the_largest_sensible_unit() {
        assert_eq!(short_age(42), "<1m");
        assert_eq!(short_age(900), "15m");
        assert_eq!(short_age(3_600), "1h");
        assert_eq!(short_age(8_040), "2h 14m");
        assert_eq!(short_age(90_000), "1d 1h");
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_accepted() {
        let path = std::env::temp_dir().join(format!("termden-jsonc-{}.jsonc", std::process::id()));
        std::fs::write(
            &path,
            "{\n  // enabled servers only\n  \"mcp\": {\n    \"a\": { \"type\": \"local\", \"enabled\": true, },\n    /* not this one */ \"b\": { \"enabled\": false },\n  },\n}\n",
        )
        .unwrap();
        let mut names = Vec::new();
        push_mcp_block(&mut names, &path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(names, ["a"]);
    }

    #[test]
    fn an_mcp_servers_wrapper_is_unwrapped() {
        let path =
            std::env::temp_dir().join(format!("termden-mcp-wrap-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"mcp":{"servers":{"atlas":{"type":"local"},"off":{"type":"local","enabled":false}}}}"#,
        )
        .unwrap();
        let mut names = Vec::new();
        push_mcp_block(&mut names, &path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(names, ["atlas"]);
    }

    #[test]
    fn disabled_opencode_mcp_servers_are_not_listed() {
        let path = std::env::temp_dir().join(format!("termden-mcp-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"mcp":{"termden":{"type":"local","enabled":true},"gone":{"type":"local","enabled":false},"plain":{"command":["x"]}}}"#,
        )
        .unwrap();
        let mut names = Vec::new();
        push_mcp_block(&mut names, &path);
        let _ = std::fs::remove_file(&path);
        names.sort();
        assert_eq!(names, ["plain", "termden"]);
    }
}
