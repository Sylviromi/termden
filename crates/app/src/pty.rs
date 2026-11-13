//! Live terminal sessions: a tmux client on a private server, parsed by
//! `alacritty_terminal`. Dropping a `Session` only detaches the client; the
//! tmux session (and the harness inside it) keeps running.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::tty::{self, Options, Shell};
use gpui::Keystroke;
use termden_core::{HarnessId, state::state_dir};

#[derive(Clone)]
pub struct Listener(async_channel::Sender<Event>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.try_send(event);
    }
}

pub struct Session {
    pub term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    thread: Option<JoinHandle<()>>,
    pub cols: u16,
    pub rows: u16,
    pub exited: bool,
    /// Last PTY output and last keystroke; together they approximate
    /// "working" until real adapters report it.
    pub last_output: Instant,
    pub last_input: Instant,
    /// Last real human activity (typing, paste, mouse). Injections on the
    /// harness's behalf do not count, so "the user is quiet" is measurable.
    pub last_human: Instant,
    /// Characters typed into the harness since the last submit. Non-zero means
    /// an unsent draft is very likely sitting in the prompt.
    pub draft: u32,
    /// Set by a bell while unfocused; cleared when the tile gets focus.
    pub attention: bool,
    /// Terminal title (tmux forwards the pane title).
    pub title: String,
}

/// Our private tmux server's config. The user's `~/.tmux.conf` is not read:
/// termden owns this server's look and behaviour.
const TMUX_CONF: &str = "\
set -g status off
set -g mouse on
set -g set-titles on
set -g set-titles-string '#T'
set -g set-clipboard on
set -g focus-events on
set -g history-limit 50000
set -g window-size latest
set -g default-terminal tmux-256color
set -s escape-time 10
set -s extended-keys on
set -as terminal-features ',xterm-256color:RGB:clipboard:title:extkeys'
";

fn tmux_conf() -> io::Result<PathBuf> {
    let path = state_dir().join("tmux.conf");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(TMUX_CONF) {
        std::fs::create_dir_all(state_dir())?;
        std::fs::write(&path, TMUX_CONF)?;
    }
    Ok(path)
}

/// PATH as the user's interactive shell sets it (tools like `claude` often
/// live in dirs only `.zshrc` adds). Computed once; call early off-thread to
/// warm it.
pub fn user_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        const MARK: &str = "__TERMDEN_PATH__";
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());
        let probe = std::process::Command::new(shell)
            .args(["-ic", &format!("printf '\\n{MARK}%s\\n' \"$PATH\"")])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output();
        probe
            .ok()
            .and_then(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .find_map(|line| line.strip_prefix(MARK).map(str::to_string))
            })
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default())
    })
}

/// Kill a harness for good (closing a tile). Detaching is just dropping the
/// `Session`.
pub fn kill_tmux_session(id: HarnessId) {
    let _ = std::process::Command::new("tmux")
        .args(["-L", &tmux_socket(), "kill-session", "-t"])
        .arg(format!("td-{}", id.raw()))
        .output();
}

/// Environment and command for a new session.
fn push_session_env(args: &mut Vec<String>, id: HarnessId, command: Option<&str>) {
    // Lets `termdenctl` inside a harness know who it is (`self`) and which
    // app instance to talk to.
    args.push("-e".into());
    args.push(format!("TERMDEN_HARNESS={}", id.raw()));
    args.push("-e".into());
    args.push(format!(
        "TERMDEN_CONTROL_SOCKET={}",
        termden_core::state::control_socket().display()
    ));
    // The PATH the user's interactive shell would give (agents run without
    // a shell), with our own bin dir first so `termdenctl` is always there.
    let own = bin_dir()
        .map(|dir| format!("{}:", dir.display()))
        .unwrap_or_default();
    args.push("-e".into());
    args.push(format!("PATH={own}{}", user_path()));
    if let Some(command) = command {
        args.push(command.into());
    }
}

/// Where `termdenctl` lives: next to our binary, unless `TERMDEN_BIN_DIR`
/// says otherwise (`dx serve` builds termden into its own directory).
pub fn bin_dir() -> Option<std::path::PathBuf> {
    match std::env::var_os("TERMDEN_BIN_DIR") {
        Some(dir) if !dir.is_empty() => Some(dir.into()),
        _ => Some(std::env::current_exe().ok()?.parent()?.to_path_buf()),
    }
}

/// Last `lines` lines of a harness's scrollback plus screen, from tmux.
pub fn capture_history(id: HarnessId, lines: u32) -> String {
    let out = std::process::Command::new("tmux")
        .args(["-L", &tmux_socket(), "capture-pane", "-p", "-J", "-t"])
        .arg(format!("td-{}", id.raw()))
        .args(["-S", &format!("-{lines}")])
        .output();
    match out {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim_end().to_string()
        }
        Ok(out) => format!("error: {}", String::from_utf8_lossy(&out.stderr).trim()),
        Err(error) => format!("error: tmux: {error}"),
    }
}

/// Name of the private tmux server. Overridable so headless captures do not
/// share sessions with the user's real instance.
pub fn tmux_socket() -> String {
    std::env::var("TERMDEN_TMUX_SOCKET").unwrap_or_else(|_| "termden".into())
}

impl Session {
    /// The prompt line under the cursor is a `/clear` (or opencode's `/new`).
    pub fn cursor_line_is_clear(&self) -> bool {
        let term = self.term.lock();
        let grid = term.grid();
        let row = &grid[grid.cursor.point.line];
        let text: String = (0..grid.columns()).map(|c| row[Column(c)].c).collect();
        let text = text.trim_end();
        text.ends_with("/clear") || text.ends_with("/new")
    }

    pub fn spawn(
        id: HarnessId,
        cwd: PathBuf,
        command: Option<&str>,
        attach_only: bool,
        cols: u16,
        rows: u16,
        cell: (f32, f32),
    ) -> io::Result<(Self, async_channel::Receiver<Event>)> {
        let (tx, rx) = async_channel::unbounded();
        let listener = Listener(tx);
        let term = Term::new(
            TermConfig::default(),
            &TermSize::new(cols as usize, rows as usize),
            listener.clone(),
        );
        let term = Arc::new(FairMutex::new(term));

        // `env -u TMUX` so launching termden from inside tmux does not trip
        // tmux's nesting guard. `-A` reattaches when the session survives us.
        let mut args: Vec<String> = [
            "-u",
            "TMUX",
            "tmux",
            "-L",
            &tmux_socket(),
            "-f",
            &tmux_conf()?.to_string_lossy(),
            "new-session",
            "-A",
            "-s",
            &format!("td-{}", id.raw()),
            "-c",
            &cwd.to_string_lossy(),
        ]
        .map(String::from)
        .to_vec();
        if attach_only {
            // Mirrors must never create sessions: a missing one would start a
            // fresh agent that outlives the mirror.
            let at = args
                .iter()
                .position(|a| a == "new-session")
                .unwrap_or(args.len());
            args.truncate(at);
            args.extend(["attach-session", "-t"].map(String::from));
            args.push(format!("td-{}", id.raw()));
        } else {
            push_session_env(&mut args, id, command);
        }

        let options = Options {
            shell: Some(Shell::new("env".into(), args)),
            working_directory: Some(cwd),
            env: HashMap::from([
                ("TERM".into(), "xterm-256color".into()),
                ("COLORTERM".into(), "truecolor".into()),
            ]),
            ..Default::default()
        };

        let pty = tty::new(&options, window_size(cols, rows, cell), id.raw() as u64)?;
        let event_loop = EventLoop::new(term.clone(), listener, pty, false, false)?;
        let sender = event_loop.channel();
        let handle = event_loop.spawn();
        // Joining drops the event loop and with it the PTY, which SIGHUPs and
        // reaps the tmux client. The tmux session itself is untouched.
        let thread = std::thread::spawn(move || drop(handle.join()));

        let now = Instant::now();
        Ok((
            Self {
                term,
                sender,
                thread: Some(thread),
                cols,
                rows,
                exited: false,
                last_output: now,
                last_input: now,
                last_human: now,
                draft: 0,
                attention: false,
                title: String::new(),
            },
            rx,
        ))
    }

    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let _ = self.sender.send(Msg::Input(bytes.into()));
    }

    /// Typed input: counts as user activity for the status heuristic and
    /// extends the pending draft.
    pub fn input(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        let bytes = bytes.into();
        self.last_input = Instant::now();
        self.last_human = self.last_input;
        self.draft = track_draft(self.draft, &bytes);
        self.write(bytes);
    }

    /// Written on the harness's behalf (mail nudges). It is activity for the
    /// status heuristic, but not human activity and never a draft: if it were,
    /// our own nudge would keep postponing the next one.
    pub fn inject(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.last_input = Instant::now();
        self.write(bytes);
    }

    pub fn paste(&mut self, text: &str) {
        let bracketed = self.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if bracketed {
            // Newlines inside a bracketed paste are newlines in the prompt, not
            // submits, so `input` would misread the draft; count it here.
            let added = text.chars().filter(|c| !c.is_whitespace()).count() as u32;
            self.input(format!("\x1b[200~{text}\x1b[201~").into_bytes());
            self.draft = self.draft.saturating_add(added);
        } else {
            self.input(text.into_bytes());
        }
    }

    /// True when the prompt line under the cursor holds nothing but prompt
    /// chrome (box borders, the agent's `❯`, padding).
    ///
    /// Backstop for `draft`: tmux keeps a harness and its unsent draft across a
    /// hot reload, but this struct's counters do not survive it.
    pub fn prompt_is_empty(&self) -> bool {
        let term = self.term.lock();
        draft_text_is_empty(&cursor_prompt_text(&term))
    }

    /// Forward a mouse event as an SGR report, if the program asked for
    /// mouse reporting (tmux does, with `mouse on`).
    ///
    /// shortcut: legacy X10 mouse encoding is not supported; every program we
    /// host sits behind tmux, which always negotiates SGR.
    pub fn mouse(&mut self, button: u8, col: u16, row: u16, pressed: bool) {
        let mode = *self.term.lock().mode();
        if !mode.intersects(TermMode::MOUSE_MODE) || !mode.contains(TermMode::SGR_MOUSE) {
            return;
        }
        let col = col.min(self.cols.saturating_sub(1));
        let row = row.min(self.rows.saturating_sub(1));
        self.input(sgr_mouse(button, col, row, pressed).into_bytes());
    }

    /// Wants motion events while a button is held.
    pub fn wants_drag(&self) -> bool {
        self.term
            .lock()
            .mode()
            .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
    }

    /// The http(s) URL under a visible cell: (start column, cells, url).
    /// shortcut: only looks at one row, so a URL wrapped onto the next line is
    /// cut there; join WRAPLINE rows if long links show up.
    pub fn url_at(&self, col: u16, row: u16) -> Option<(u16, u16, String)> {
        let term = self.term.lock();
        let line = &term.grid()[Line(row as i32)];
        let text: Vec<char> = (0..term.columns()).map(|c| line[Column(c)].c).collect();
        url_at(&text, col as usize).map(|(start, len, url)| (start as u16, len as u16, url))
    }

    /// The visible screen as plain text, trailing blanks trimmed.
    pub fn screen_text(&self) -> String {
        let term = self.term.lock();
        let grid = term.grid();
        let mut out = String::new();
        for line in 0..term.screen_lines() {
            let row = &grid[Line(line as i32)];
            let text: String = (0..term.columns()).map(|c| row[Column(c)].c).collect();
            out.push_str(text.trim_end());
            out.push('\n');
        }
        out.trim_end().to_string()
    }

    /// Stop the client and wait until the PTY child is reaped. Used before a
    /// hot reload so no orphaned tmux clients pin old window sizes.
    pub fn shutdown_and_wait(mut self) {
        let _ = self.sender.send(Msg::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16, cell: (f32, f32)) {
        if (cols, rows) == (self.cols, self.rows) || self.exited {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term
            .lock()
            .resize(TermSize::new(cols as usize, rows as usize));
        let _ = self.sender.send(Msg::Resize(window_size(cols, rows, cell)));
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

fn window_size(cols: u16, rows: u16, (w, h): (f32, f32)) -> WindowSize {
    WindowSize {
        num_lines: rows,
        num_cols: cols,
        cell_width: w as u16,
        cell_height: h as u16,
    }
}

/// The http(s) URL in `line` covering index `col` (one char per cell), with
/// its start index and length.
fn url_at(line: &[char], col: usize) -> Option<(usize, usize, String)> {
    let text: String = line.iter().collect();
    let chars = |s: &str| s.chars().count();
    for scheme in ["https://", "http://"] {
        for (byte, _) in text.match_indices(scheme) {
            let start = chars(&text[..byte]);
            let tail: String = line[start..]
                .iter()
                .take_while(|c| !c.is_whitespace() && !"\"'<>`│".contains(**c))
                .collect();
            let url = tail.trim_end_matches(['.', ',', ';', ':', ')', ']', '}', '!', '?']);
            if (start..start + chars(url)).contains(&col) {
                return Some((start, chars(url), url.to_string()));
            }
        }
    }
    None
}

fn sgr_mouse(button: u8, col: u16, row: u16, pressed: bool) -> String {
    let end = if pressed { 'M' } else { 'm' };
    format!("\x1b[<{button};{};{}{end}", col + 1, row + 1)
}

/// Prompt decoration an agent's TUI puts before its input: box borders and the
/// usual prompt glyphs. Only *leading* chrome is dropped, so a draft that
/// starts with one of them still counts.
const PROMPT_CHROME: &[char] = &[
    ' ', '\u{a0}', '\t', '│', '┃', '║', '╭', '╮', '╰', '╯', '─', '━', '▀', '▄', '╹', '╻', '❯', '>',
    '$', '#', '%', ':', '·', '∙',
];

fn draft_text_is_empty(text: &str) -> bool {
    text.trim_start_matches(PROMPT_CHROME).is_empty()
}

/// The grid text on the cursor's line, left of the cursor.
fn cursor_prompt_text<L: EventListener>(term: &Term<L>) -> String {
    let grid = term.grid();
    let point = grid.cursor.point;
    let row = &grid[point.line];
    (0..point.column.0.min(term.columns()))
        .map(|col| row[Column(col)].c)
        .collect()
}

/// Advance a pending-draft counter by the bytes just handed to the PTY.
///
/// Enter submits (and clears the prompt), a lone Escape or the control keys
/// that wipe an agent prompt clear it, and backspace takes a character back.
/// Escape *sequences* (arrows, modified keys) and other control bytes leave the
/// count alone: they never remove text we already counted.
fn track_draft(draft: u32, bytes: &[u8]) -> u32 {
    let mut draft = draft;
    for &byte in bytes {
        match byte {
            b'\r' | b'\n' | 0x03 | 0x15 => draft = 0,
            0x1b if bytes.len() == 1 => draft = 0,
            0x1b => return draft,
            0x7f | 0x08 => draft = draft.saturating_sub(1),
            b' '..=b'~' => draft = draft.saturating_add(1),
            // One count per UTF-8 lead byte; continuation bytes add nothing.
            0xc0..=0xff => draft = draft.saturating_add(1),
            _ => {}
        }
    }
    draft
}

/// SGR button codes.
pub const MOUSE_LEFT: u8 = 0;
pub const MOUSE_MIDDLE: u8 = 1;
pub const MOUSE_RIGHT: u8 = 2;
pub const MOUSE_DRAG: u8 = 32;
pub const WHEEL_UP: u8 = 64;
pub const WHEEL_DOWN: u8 = 65;

/// Translate a GPUI keystroke into the bytes an xterm would send.
///
/// shortcut: no kitty keyboard protocol and no modified F1-F12; add them when a
/// harness needs them.
pub fn key_bytes(key: &Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let m = &key.modifiers;
    if m.platform {
        return None;
    }

    // xterm modifier parameter: 1 + shift + 2*alt + 4*ctrl.
    let modp = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let cursor = |c: char| {
        if modp > 1 {
            format!("\x1b[1;{modp}{c}")
        } else if app_cursor {
            format!("\x1bO{c}")
        } else {
            format!("\x1b[{c}")
        }
    };
    let tilde = |n: u8| {
        if modp > 1 {
            format!("\x1b[{n};{modp}~")
        } else {
            format!("\x1b[{n}~")
        }
    };
    let named = match key.key.as_str() {
        // CSI u for modified Enter (shift+enter = newline in agent TUIs);
        // tmux understands it with `extended-keys on`.
        "enter" if m.shift => Some("\x1b[13;2u".into()),
        "enter" if m.control => Some("\x1b[13;5u".into()),
        "enter" => Some("\r".to_string()),
        "backspace" => Some("\x7f".into()),
        "tab" if m.shift => Some("\x1b[Z".into()),
        "tab" => Some("\t".into()),
        "escape" => Some("\x1b".into()),
        "up" => Some(cursor('A')),
        "down" => Some(cursor('B')),
        "right" => Some(cursor('C')),
        "left" => Some(cursor('D')),
        "home" => Some(cursor('H')),
        "end" => Some(cursor('F')),
        "pageup" => Some(tilde(5)),
        "pagedown" => Some(tilde(6)),
        "insert" => Some(tilde(2)),
        "delete" => Some(tilde(3)),
        "f1" => Some("\x1bOP".into()),
        "f2" => Some("\x1bOQ".into()),
        "f3" => Some("\x1bOR".into()),
        "f4" => Some("\x1bOS".into()),
        f if f.len() <= 3 && f.starts_with('f') => match &f[1..] {
            "5" => Some("\x1b[15~".into()),
            "6" => Some("\x1b[17~".into()),
            "7" => Some("\x1b[18~".into()),
            "8" => Some("\x1b[19~".into()),
            "9" => Some("\x1b[20~".into()),
            "10" => Some("\x1b[21~".into()),
            "11" => Some("\x1b[23~".into()),
            "12" => Some("\x1b[24~".into()),
            _ => None,
        },
        _ => None,
    };

    let mut out = if let Some(seq) = named {
        seq.into_bytes()
    } else if m.control {
        let byte = match key.key.as_str() {
            "space" | "@" | "2" => 0,
            k if k.len() == 1 => match k.as_bytes()[0] {
                c @ b'a'..=b'z' => c - b'a' + 1,
                b'[' | b'3' => 0x1b,
                b'\\' | b'4' => 0x1c,
                b']' | b'5' => 0x1d,
                b'^' | b'6' => 0x1e,
                b'_' | b'-' | b'7' => 0x1f,
                _ => return None,
            },
            _ => return None,
        };
        vec![byte]
    } else if key.key == "space" {
        b" ".to_vec()
    } else {
        // Alt combos often arrive without `key_char`; fall back to the bare key.
        key.key_char
            .clone()
            .or_else(|| (key.key.chars().count() == 1).then(|| key.key.clone()))?
            .into_bytes()
    };

    // Modified cursor/tilde keys already carry alt in their parameter.
    let encoded_alt = modp > 1
        && matches!(
            key.key.as_str(),
            "up" | "down"
                | "left"
                | "right"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
                | "insert"
                | "delete"
        );
    if m.alt && !encoded_alt {
        out.insert(0, 0x1b);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(text: &str) -> Vec<u8> {
        key_bytes(&Keystroke::parse(text).unwrap(), false).unwrap()
    }

    #[test]
    fn translates_common_keys() {
        assert_eq!(bytes("enter"), b"\r");
        assert_eq!(bytes("shift-enter"), b"\x1b[13;2u");
        assert_eq!(bytes("ctrl-c"), [3]);
        assert_eq!(bytes("alt-b"), b"\x1bb");
        assert_eq!(bytes("shift-tab"), b"\x1b[Z");
        assert_eq!(bytes("up"), b"\x1b[A");
        assert_eq!(bytes("ctrl-left"), b"\x1b[1;5D");
        assert_eq!(bytes("alt-right"), b"\x1b[1;3C");
        assert_eq!(bytes("ctrl-shift-delete"), b"\x1b[3;6~");
        assert_eq!(
            key_bytes(&Keystroke::parse("up").unwrap(), true).unwrap(),
            b"\x1bOA"
        );
    }

    #[test]
    fn finds_url_under_cell() {
        let line: Vec<char> = "  see https://x.io/a?b=1. ok http://127.0.0.1:9/t/ │"
            .chars()
            .collect();
        let url = |col| url_at(&line, col).map(|(_, _, url)| url);
        assert_eq!(
            url_at(&line, 10),
            Some((6, 18, "https://x.io/a?b=1".into()))
        );
        assert_eq!(url(6).as_deref(), Some("https://x.io/a?b=1"));
        assert_eq!(url_at(&line, 24), None);
        assert_eq!(url_at(&line, 2), None);
        assert_eq!(url(30).as_deref(), Some("http://127.0.0.1:9/t/"));
    }

    #[test]
    fn encodes_sgr_mouse() {
        assert_eq!(sgr_mouse(MOUSE_LEFT, 0, 0, true), "\x1b[<0;1;1M");
        assert_eq!(sgr_mouse(WHEEL_DOWN, 9, 4, true), "\x1b[<65;10;5M");
        assert_eq!(
            sgr_mouse(MOUSE_LEFT | MOUSE_DRAG, 2, 3, false),
            "\x1b[<32;3;4m"
        );
    }

    #[test]
    fn tracks_an_unsent_draft() {
        let mut draft = 0;
        draft = track_draft(draft, b"fix the mail bug");
        assert_eq!(draft, 16);
        // Navigation and shortcuts are not text and must not clear it.
        draft = track_draft(draft, b"\x1b[A");
        draft = track_draft(draft, b"\x1b[13;2u");
        assert_eq!(draft, 16);
        // Backspace takes a character back.
        draft = track_draft(draft, b"\x7f");
        assert_eq!(draft, 15);
        // Enter submits and clears.
        assert_eq!(track_draft(draft, b"\r"), 0);
        // A lone Escape wipes an agent prompt.
        assert_eq!(track_draft(3, b"\x1b"), 0);
        // Ctrl-C / Ctrl-U clear it too.
        assert_eq!(track_draft(3, &[0x03]), 0);
        assert_eq!(track_draft(3, &[0x15]), 0);
        // Non-ASCII text counts once per character.
        assert_eq!(track_draft(0, "héllo".as_bytes()), 5);
    }

    #[test]
    fn reads_an_empty_prompt_from_chrome() {
        // Claude Code: `❯ ` with the cursor right after it.
        assert!(draft_text_is_empty("❯ "));
        assert!(!draft_text_is_empty("❯ fix it"));
        // opencode: a box border and padding.
        assert!(draft_text_is_empty("│    "));
        assert!(!draft_text_is_empty("│ fix it"));
        // A draft that starts with a prompt-ish glyph still reads as a draft.
        assert!(!draft_text_is_empty("❯ > fix it"));
        assert!(draft_text_is_empty(""));
    }

    #[test]
    fn reads_the_prompt_line_from_a_real_grid() {
        use alacritty_terminal::vte::ansi::Processor;

        let (tx, _rx) = async_channel::unbounded();
        let mut term: Term<Listener> =
            Term::new(TermConfig::default(), &TermSize::new(40, 8), Listener(tx));
        let mut parser: Processor = Processor::new();
        let mut feed =
            |term: &mut Term<Listener>, text: &str| parser.advance(term, text.as_bytes());

        // Claude Code's idle prompt: a rule, `❯ ` and the cursor.
        feed(&mut term, "─────\r\n❯ ");
        assert_eq!(cursor_prompt_text(&term), "❯ ");
        assert!(draft_text_is_empty(&cursor_prompt_text(&term)));
        // Typing puts text left of the cursor.
        feed(&mut term, "fix mail");
        assert_eq!(cursor_prompt_text(&term), "❯ fix mail");
        assert!(!draft_text_is_empty(&cursor_prompt_text(&term)));
        // A draft edited back to nothing is a prompt again.
        feed(&mut term, "\x1b[8D\x1b[K");
        assert_eq!(cursor_prompt_text(&term), "❯ ");
    }
}
