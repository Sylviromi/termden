//! Local HTTP server on 127.0.0.1 behind the shared results pages
//! (`showcase.rs`). The agent posts the URL in its reply and the user
//! Ctrl+clicks it.
//!
//! Access control is the random token in every path; the server only binds
//! to loopback. Port and token are kept beside the control socket so links
//! survive restarts.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use gpui::{Context, Hsla, Rgba};
use termden_core::state::control_socket;

use crate::state::TermdenApp;

impl TermdenApp {
    /// Answer one request on the app thread.
    fn route(&mut self, method: &str, path: &str) -> Reply {
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        match (method, parts[0]) {
            ("GET", "") => Reply::Redirect("r/".into()),
            ("GET", "r") => {
                // Pages link relatively, so they need their trailing slash.
                if parts.len() <= 2 && !path.ends_with('/') {
                    return Reply::Redirect(format!("{}/", path.trim_start_matches('/')));
                }
                self.showcase_route(&parts)
            }
            _ => Reply::NotFound,
        }
    }
}

pub(crate) enum Reply {
    Page(String),
    File(PathBuf),
    /// Redirect to another path under the token (adds a trailing slash).
    Redirect(String),
    NotFound,
}

type Request = (String, String, async_channel::Sender<Reply>);

fn url_file() -> PathBuf {
    control_socket().with_file_name("choices.url")
}

/// The results server's base URL.
pub fn url() -> Option<String> {
    let saved = fs::read_to_string(url_file()).ok()?;
    let (port, token) = saved.trim().split_once(' ')?;
    Some(format!("http://127.0.0.1:{port}/{token}/"))
}

pub fn listen(cx: &mut Context<TermdenApp>) {
    let (tx, rx) = async_channel::unbounded::<Request>();
    std::thread::spawn(move || {
        let saved = fs::read_to_string(url_file()).unwrap_or_default();
        let mut saved = saved.split_whitespace();
        let port: u16 = saved.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let token = saved
            .next()
            .filter(|t| t.len() == 4)
            .map(String::from)
            .unwrap_or_else(new_token);
        // The old instance may still hold the port for a moment during a hot reload.
        let listener = (port != 0)
            .then(|| {
                (0..30).find_map(|_| {
                    TcpListener::bind(("127.0.0.1", port))
                        .inspect_err(|_| std::thread::sleep(Duration::from_millis(100)))
                        .ok()
                })
            })
            .flatten()
            .or_else(|| TcpListener::bind(("127.0.0.1", 0)).ok());
        let Some(listener) = listener else {
            eprintln!("termden: could not start the results server");
            return;
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let _ = fs::write(url_file(), format!("{port} {token}\n"));
        for stream in listener.incoming().flatten() {
            let (tx, token) = (tx.clone(), token.clone());
            std::thread::spawn(move || serve(stream, &token, tx));
        }
    });
    cx.spawn(async move |this, cx| {
        while let Ok((method, path, reply)) = rx.recv().await {
            let out = this
                .update(cx, |this, cx| {
                    let out = this.route(method.as_str(), path.as_str());
                    cx.notify();
                    out
                })
                .unwrap_or(Reply::NotFound);
            let _ = reply.send(out).await;
        }
    })
    .detach();
}

fn new_token() -> String {
    let mut bytes = [0u8; 2];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `GET /token/rest?query HTTP/1.1` -> (method, "/rest"), if the token matches.
fn request_target(line: &str, token: &str) -> Option<(String, String)> {
    let mut words = line.split_whitespace();
    let method = words.next()?.to_string();
    let path = words.next()?.split('?').next()?;
    let rest = path.strip_prefix('/')?.strip_prefix(token)?;
    (rest.is_empty() || rest.starts_with('/')).then(|| (method, rest.to_string()))
}

fn serve(stream: TcpStream, token: &str, tx: async_channel::Sender<Request>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    // Drain headers; no route reads a body.
    let mut header = String::new();
    while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
        header.clear();
    }
    let reply = match request_target(&line, token) {
        Some((method, path)) => {
            let (reply_tx, reply_rx) = async_channel::bounded(1);
            if tx.send_blocking((method, path, reply_tx)).is_err() {
                return;
            }
            reply_rx.recv_blocking().unwrap_or(Reply::NotFound)
        }
        None => Reply::NotFound,
    };
    let sandbox = matches!(reply, Reply::File(_));
    let (status, kind, body, location) = match reply {
        Reply::Page(html) => ("200 OK", "text/html; charset=utf-8", html.into(), None),
        // shortcut: files are read whole into memory; stream them if agents
        // start sharing very large videos.
        Reply::File(path) => match fs::read(&path) {
            Ok(bytes) => ("200 OK", content_type(&path), bytes, None),
            Err(_) => ("404 Not Found", "text/plain", b"not found".to_vec(), None),
        },
        Reply::Redirect(rest) => (
            "303 See Other",
            "text/plain",
            Vec::new(),
            Some(format!("/{token}/{rest}")),
        ),
        Reply::NotFound => ("404 Not Found", "text/plain", b"not found".to_vec(), None),
    };
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(location) = location {
        head.push_str(&format!("Location: {location}\r\n"));
    }
    if sandbox {
        // Agent files (an SVG opened full size, say) must not run scripts on
        // this origin, where they could answer questions.
        head.push_str("Content-Security-Policy: sandbox\r\n");
    }
    head.push_str("\r\n");
    let mut stream = stream;
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

/// Content type served for a file, by extension. Agent-made HTML is sent as
/// plain text: it must not run scripts on the token-guarded origin.
pub(crate) fn content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("mp4" | "m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("mp3") => "audio/mpeg",
        Some("ogg" | "oga") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("pdf") => "application/pdf",
        Some("txt" | "md" | "log" | "csv" | "json" | "toml" | "yaml" | "yml" | "html" | "htm") => {
            "text/plain; charset=utf-8"
        }
        _ => "application/octet-stream",
    }
}

pub(crate) fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex(color: Hsla) -> String {
    let c = Rgba::from(color);
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

/// A full HTML page in the current theme's colours; `head` adds to <head>.
pub(crate) fn document(app: &TermdenApp, title: &str, head: &str, body: &str) -> String {
    let t = &app.theme;
    let vars = format!(
        ":root{{--crust:{};--mantle:{};--base:{};--surface:{};--overlay:{};--subtext:{};--text:{};--accent:{};--red:{};--green:{};--ui:'{}',system-ui,sans-serif}}",
        hex(t.crust),
        hex(t.mantle),
        hex(t.base),
        hex(t.surface0),
        hex(t.overlay),
        hex(t.subtext),
        hex(t.text),
        hex(t.accent(1)),
        hex(t.red),
        hex(t.green),
        t.ui_font,
    );
    format!(
        "<!DOCTYPE html><html><head><meta charset=utf-8><title>termden · {}</title>{head}<style>{vars}{CSS}</style></head><body>{body}</body></html>",
        esc(title)
    )
}

const CSS: &str = "
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:var(--ui);background:var(--base);color:var(--text);min-height:100vh;display:flex;flex-direction:column;line-height:1.5}
header{display:flex;align-items:center;gap:1rem;padding:.9rem 1.75rem;background:var(--mantle);border-bottom:1px solid color-mix(in srgb,var(--text) 8%,transparent)}
header>div{flex:1}
h1{font-size:1.1rem;font-weight:600}
.meta{font-size:.78rem;color:var(--overlay)}
main{flex:1;padding:1.75rem;max-width:1400px;width:100%;margin:0 auto}
.question{color:var(--subtext);margin-bottom:1.25rem}
h3{font-size:.95rem;font-weight:500}
.empty{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:.4rem;text-align:center;color:var(--subtext)}
.empty h2{color:var(--text);font-size:1.2rem;font-weight:600}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_target_checks_token() {
        let t = "abc123";
        assert_eq!(
            request_target("GET /abc123/ HTTP/1.1\r\n", t),
            Some(("GET".into(), "/".into()))
        );
        assert_eq!(
            request_target("POST /abc123/pick/ff/2?x=1 HTTP/1.1", t),
            Some(("POST".into(), "/pick/ff/2".into()))
        );
        assert_eq!(
            request_target("GET /abc123 HTTP/1.1", t),
            Some(("GET".into(), "".into()))
        );
        assert_eq!(request_target("GET /abc1234/ HTTP/1.1", t), None);
        assert_eq!(request_target("GET /other/ HTTP/1.1", t), None);
        assert_eq!(request_target("GET / HTTP/1.1", t), None);
    }

    #[test]
    fn escapes_html() {
        assert_eq!(
            esc("<a href=\"x\">&</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
    }
}
