//! Agent-to-user questions, answered in the browser. A tiny HTTP server on
//! 127.0.0.1 serves the front question as a page in termden's colours; a pick
//! is mailed back to the asking agent. The agent posts the URL in its reply
//! and the user Ctrl+clicks it; the sidebar banner focuses that agent.
//!
//! Access control is the random token in every path; the server only binds
//! to loopback. Port and token are kept beside the control socket so links
//! survive hot reloads.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use gpui::{AnyElement, Context, FontWeight, Hsla, Rgba, div, prelude::*, px};
use serde_json::Value;
use termden_core::state::control_socket;
use termden_core::{Choice, ChoiceOption, HarnessId};

use crate::state::TermdenApp;
use crate::widgets::icons;

pub fn parse(from: HarnessId, body: &Value) -> Result<Choice, String> {
    let text = |v: &Value, max: usize| {
        v.as_str()
            .unwrap_or("")
            .trim()
            .chars()
            .take(max)
            .collect::<String>()
    };
    let title = text(&body["title"], 80);
    if title.is_empty() {
        return Err("missing title".into());
    }
    let options: Vec<_> = body["options"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| ChoiceOption {
            label: text(&o["label"], 160),
            description: text(&o["description"], 240),
            image: o["image"]
                .as_str()
                .map(PathBuf::from)
                .filter(|p| p.is_absolute() && p.is_file()),
        })
        .collect();
    if !(2..=8).contains(&options.len()) || options.iter().any(|o| o.label.is_empty()) {
        return Err("need 2-8 options, each with a label".into());
    }
    Ok(Choice {
        from,
        title,
        question: text(&body["question"], 300),
        options,
    })
}

impl TermdenApp {
    pub fn add_choice(&mut self, choice: Choice) {
        self.log_harness(choice.from, format!("needs your choice: {}", choice.title));
        self.choices.push(choice);
        self.save();
    }

    /// Mail the pick to the asking agent and move on to the next question.
    pub fn answer_choice(&mut self, index: usize) {
        if self.choices.is_empty() || index >= self.choices[0].options.len() {
            return;
        }
        let choice = self.choices.remove(0);
        let option = &choice.options[index];
        let text = format!(
            "I chose option {} \"{}\" for \"{}\".",
            index + 1,
            option.label,
            choice.title
        );
        self.send_mail(None, choice.from, text);
        self.save();
    }

    /// Drop the front question without picking; the asking agent is told.
    pub fn skip_choice(&mut self) {
        if self.choices.is_empty() {
            return;
        }
        let choice = self.choices.remove(0);
        let text = format!(
            "I skipped your question \"{}\" without choosing; decide yourself or ask again later.",
            choice.title
        );
        self.send_mail(None, choice.from, text);
        self.save();
    }

    /// Answer one picker request on the app thread.
    fn route(&mut self, method: &str, path: &str) -> Reply {
        let front = self.choices.first().map(key);
        if path.is_empty() {
            return Reply::Back; // relative links need the trailing slash
        }
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        if parts[0] == "r" && method == "GET" {
            // Pages link relatively, so they need their trailing slash.
            if parts.len() <= 2 && !path.ends_with('/') {
                return Reply::Redirect(format!("{}/", path.trim_start_matches('/')));
            }
            return self.showcase_route(&parts);
        }
        match (method, parts.as_slice()) {
            ("GET", [""]) => Reply::Page(page(self)),
            ("GET", ["img", i]) => i
                .parse::<usize>()
                .ok()
                .and_then(|i| self.choices.first()?.options.get(i)?.image.clone())
                .map_or(Reply::NotFound, Reply::File),
            ("POST", ["pick", k, i]) => {
                if front.as_deref() == Some(*k) {
                    if let Ok(i) = i.parse() {
                        self.answer_choice(i);
                    }
                }
                Reply::Back
            }
            ("POST", ["skip", k]) => {
                if front.as_deref() == Some(*k) {
                    self.skip_choice();
                }
                Reply::Back
            }
            _ => Reply::NotFound,
        }
    }
}

/// Identifies a question so a stale tab cannot answer the next one.
fn key(choice: &Choice) -> String {
    let mut h = DefaultHasher::new();
    (choice.from.raw(), &choice.title, &choice.question).hash(&mut h);
    choice.options.iter().for_each(|o| o.label.hash(&mut h));
    format!("{:x}", h.finish())
}

pub(crate) enum Reply {
    Page(String),
    File(PathBuf),
    /// Redirect back to the page after a POST.
    Back,
    /// Redirect to another path under the token (adds a trailing slash).
    Redirect(String),
    NotFound,
}

type Request = (String, String, async_channel::Sender<Reply>);

fn url_file() -> PathBuf {
    control_socket().with_file_name("choices.url")
}

/// The picker page URL of the running server.
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
            .filter(|t| t.len() >= 32)
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
            eprintln!("termden: could not start the choice server");
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
    let mut bytes = [0u8; 16];
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
        Reply::Back => (
            "303 See Other",
            "text/plain",
            Vec::new(),
            Some(format!("/{token}/")),
        ),
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

/// The picker page for the front question, in the current theme's colours.
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

fn page(app: &TermdenApp) -> String {
    let body = match app.choices.first() {
        None => "<main class=empty><div class=done>✓</div><h2>Nothing to choose right now</h2>\
                 <p>This page refreshes when an agent asks again.</p></main>"
            .to_string(),
        Some(choice) => {
            let asker = app
                .harness(choice.from)
                .map_or("An agent".into(), |(_, h)| h.name.clone());
            let k = key(choice);
            let images = choice.options.iter().any(|o| o.image.is_some());
            let cards: String = choice
                .options
                .iter()
                .enumerate()
                .map(|(i, o)| {
                    let preview = match (&o.image, images) {
                        (Some(_), _) => format!("<div class=img><img src=\"img/{i}?k={k}\"></div>"),
                        (None, true) => format!("<div class=img><span>{}</span></div>", i + 1),
                        (None, false) => String::new(),
                    };
                    format!(
                        "<form method=post action=\"pick/{k}/{i}\"><button class=card>{preview}\
                         <div class=body><span class=num>{n}</span><div><h3>{label}</h3><p>{desc}</p></div></div>\
                         </button></form>",
                        n = i + 1,
                        label = esc(&o.label),
                        desc = esc(&o.description),
                    )
                })
                .collect();
            let more = match app.choices.len() {
                1 => String::new(),
                n => format!(" · 1 of {n}"),
            };
            format!(
                "<header><div><h1>{title}</h1><div class=meta>{asker} asks{more}</div></div>\
                 <form method=post action=\"skip/{k}\"><button class=skip>Skip</button></form></header>\
                 <main>{question}<div class=\"cards{rows}\">{cards}</div></main>\
                 <footer>Click an option or press its number</footer>\
                 <script>addEventListener('keydown',e=>{{const f=document.querySelectorAll('.cards form')[e.key-1];if(f)f.submit()}})</script>",
                title = esc(&choice.title),
                asker = esc(&asker),
                question = (!choice.question.is_empty())
                    .then(|| format!("<p class=question>{}</p>", esc(&choice.question)))
                    .unwrap_or_default(),
                rows = if images { "" } else { " rows" },
            )
        }
    };
    let refresh = if app.choices.is_empty() {
        "<meta http-equiv=refresh content=3>"
    } else {
        ""
    };
    document(app, "choose", refresh, &body)
}

const CSS: &str = "
*{box-sizing:border-box;margin:0;padding:0}
body{font-family:var(--ui);background:var(--base);color:var(--text);min-height:100vh;display:flex;flex-direction:column;line-height:1.5}
header{display:flex;align-items:center;gap:1rem;padding:.9rem 1.75rem;background:var(--mantle);border-bottom:1px solid color-mix(in srgb,var(--text) 8%,transparent)}
header>div{flex:1}
h1{font-size:1.1rem;font-weight:600}
.meta{font-size:.78rem;color:var(--overlay)}
.skip{font:inherit;font-size:.82rem;color:var(--overlay);background:none;border:1px solid color-mix(in srgb,var(--text) 12%,transparent);border-radius:8px;padding:.35rem .9rem;cursor:pointer}
.skip:hover{color:var(--red);border-color:var(--red)}
main{flex:1;padding:1.75rem;max-width:1400px;width:100%;margin:0 auto}
.question{color:var(--subtext);margin-bottom:1.25rem}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem}
.cards.rows{grid-template-columns:1fr;max-width:760px}
.card{all:unset;box-sizing:border-box;display:flex;flex-direction:column;width:100%;height:100%;background:var(--mantle);border:1.5px solid color-mix(in srgb,var(--text) 9%,transparent);border-radius:12px;overflow:hidden;cursor:pointer;transition:all .15s ease}
.card:hover,.card:focus-visible{border-color:var(--accent);background:color-mix(in srgb,var(--accent) 8%,var(--mantle));transform:translateY(-2px);box-shadow:0 6px 18px rgba(0,0,0,.25)}
.img{aspect-ratio:4/3;max-height:50vh;background:var(--crust);display:flex;align-items:center;justify-content:center}
.img img{width:100%;height:100%;object-fit:contain}
.img span{font-size:2.5rem;color:var(--overlay);opacity:.5}
.body{display:flex;gap:.75rem;padding:.9rem 1rem;align-items:flex-start}
.num{flex-shrink:0;width:1.6rem;height:1.6rem;border-radius:6px;display:flex;align-items:center;justify-content:center;font-size:.8rem;font-weight:600;color:var(--accent);background:color-mix(in srgb,var(--accent) 18%,transparent)}
.card:hover .num{background:var(--accent);color:var(--crust)}
h3{font-size:.95rem;font-weight:500}
.body p{font-size:.85rem;color:var(--subtext)}
footer{padding:.6rem 1.75rem;text-align:center;font-size:.75rem;color:var(--overlay);background:var(--mantle);border-top:1px solid color-mix(in srgb,var(--text) 6%,transparent)}
.empty{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:.4rem;text-align:center;color:var(--subtext)}
.empty h2{color:var(--text);font-size:1.2rem;font-weight:600}
.done{width:3rem;height:3rem;border-radius:50%;display:flex;align-items:center;justify-content:center;font-size:1.4rem;color:var(--green);background:color-mix(in srgb,var(--green) 15%,transparent)}
";

/// "X is waiting" strip; clicking it focuses the asking agent, whose
/// terminal holds the picker link.
pub fn banner(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    let choice = app.choices.first()?;
    let theme = &app.theme;
    let from = choice.from;
    let asker = app
        .harness(from)
        .map_or("An agent".into(), |(_, h)| h.name.clone());
    let more = app.choices.len() - 1;
    Some(
        div()
            .id("choice-banner")
            .mx(px(9.))
            .mb(px(4.))
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(theme.blue.opacity(0.14))
            .border_1()
            .border_color(theme.blue.opacity(0.3))
            .cursor_pointer()
            .hover(|s| s.opacity(0.85))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.focus_harness(from);
                cx.notify();
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_size(theme.ui(10.5))
                    .text_color(theme.blue)
                    .child(format!(
                        "{asker} is waiting: {}{}",
                        choice.title,
                        if more > 0 {
                            format!(" (+{more})")
                        } else {
                            String::new()
                        }
                    )),
            )
            .child(
                div()
                    .text_size(theme.ui(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.blue)
                    .child("Choose"),
            )
            .child(
                crate::widgets::icon_button("choice-skip", icons::CLOSE, theme)
                    .size(px(20.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.skip_choice();
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .into_any_element(),
    )
}

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
