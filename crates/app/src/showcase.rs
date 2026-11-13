//! Results agents show the user (images, videos, audio, files): a gallery
//! page on the picker's local server (`choices.rs`), view only. The agent gets
//! the page URL back and posts it; nothing is mailed back.
//!
//! Only files an agent listed are served, behind the server's token, and with
//! a sandbox CSP so e.g. an SVG opened full size cannot run scripts.

use std::path::PathBuf;

use serde_json::Value;
use termden_core::{HarnessId, ShowItem, Showcase};

use crate::choices::{Reply, content_type, document, esc};
use crate::state::TermdenApp;

/// Result sets kept (and persisted); older ones drop off.
const KEEP: usize = 20;
const MAX_ITEMS: usize = 50;

/// Validate a `show` request: a title and 1-50 existing files by absolute path.
pub fn parse(body: &Value) -> Result<(String, String, Vec<ShowItem>), String> {
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
    let raw = body["items"].as_array().cloned().unwrap_or_default();
    if !(1..=MAX_ITEMS).contains(&raw.len()) {
        return Err(format!("need 1-{MAX_ITEMS} items"));
    }
    let mut items = Vec::new();
    let mut missing = Vec::new();
    for item in &raw {
        let path = PathBuf::from(text(&item["path"], 4096));
        if !path.is_absolute() || !path.is_file() {
            missing.push(path.display().to_string());
            continue;
        }
        items.push(ShowItem {
            path,
            caption: text(&item["caption"], 160),
            description: text(&item["description"], 400),
        });
    }
    if !missing.is_empty() {
        return Err(format!(
            "not an absolute path to an existing file: {}",
            missing.join(", ")
        ));
    }
    Ok((title, text(&body["description"], 600), items))
}

impl TermdenApp {
    /// Store a result set and return its id (`r/<id>/` on the results page).
    pub fn add_showcase(
        &mut self,
        from: HarnessId,
        title: String,
        description: String,
        items: Vec<ShowItem>,
    ) -> u64 {
        let id = self.showcases.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let from_name = self
            .harness(from)
            .map_or("An agent".into(), |(_, h)| h.name.clone());
        let n = items.len();
        self.log_harness(
            from,
            format!(
                "shared {n} result{}: {title}",
                if n == 1 { "" } else { "s" }
            ),
        );
        self.showcases.push(Showcase {
            id,
            from,
            from_name,
            title,
            description,
            time: crate::state::clock(0),
            items,
        });
        let excess = self.showcases.len().saturating_sub(KEEP);
        self.showcases.drain(..excess);
        self.save();
        id
    }

    /// GET routes under `r/`: the index, one set, and its files.
    pub(crate) fn showcase_route(&self, parts: &[&str]) -> Reply {
        match parts {
            ["r"] => Reply::Page(index(self)),
            ["r", id] => self
                .find_showcase(id)
                .map_or(Reply::NotFound, |s| Reply::Page(gallery(self, s))),
            ["r", id, "f", i] => self
                .find_showcase(id)
                .and_then(|s| s.items.get(i.parse::<usize>().ok()?))
                .map_or(Reply::NotFound, |item| Reply::File(item.path.clone())),
            _ => Reply::NotFound,
        }
    }

    fn find_showcase(&self, id: &str) -> Option<&Showcase> {
        let id: u64 = id.parse().ok()?;
        self.showcases.iter().find(|s| s.id == id)
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Every kept set, newest first, with the first image as a thumbnail.
fn index(app: &TermdenApp) -> String {
    let rows: String = app
        .showcases
        .iter()
        .rev()
        .map(|s| {
            let thumb = s
                .items
                .iter()
                .position(|i| content_type(&i.path).starts_with("image/"))
                .map(|i| format!("<img src=\"{}/f/{i}\">", s.id))
                .unwrap_or_default();
            format!(
                "<a class=set href=\"{id}/\"><div class=thumb>{thumb}</div><div><h3>{title}</h3>\
                 <p>{from} · {time} · {n} item{s}</p></div></a>",
                id = s.id,
                title = esc(&s.title),
                from = esc(&s.from_name),
                time = esc(&s.time),
                n = s.items.len(),
                s = plural(s.items.len()),
            )
        })
        .collect();
    let body = if rows.is_empty() {
        "<main class=empty><h2>Nothing shared yet</h2>\
         <p>Agents share results with termden_show_results.</p></main>"
            .to_string()
    } else {
        format!(
            "<header><div><h1>Shared results</h1><div class=meta>latest {} kept</div></div></header>\
             <main><div class=sets>{rows}</div></main>",
            KEEP
        )
    };
    document(app, "results", SHOW_CSS, &body)
}

/// One result set as a gallery: images large (click for full size), video
/// and audio playable, other files as open/download cards.
fn gallery(app: &TermdenApp, s: &Showcase) -> String {
    let items: String = s
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let src = format!("f/{i}");
            let name = item
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let kind = content_type(&item.path);
            let media = if kind.starts_with("image/") {
                format!("<a class=media href=\"{src}\" target=_blank><img src=\"{src}\"></a>")
            } else if kind.starts_with("video/") {
                format!("<div class=media><video controls preload=metadata src=\"{src}\"></video></div>")
            } else if kind.starts_with("audio/") {
                format!("<div class=\"media audio\"><audio controls preload=metadata src=\"{src}\"></audio></div>")
            } else {
                format!(
                    "<a class=\"media file\" href=\"{src}\" target=_blank><span>{}</span></a>",
                    esc(&item
                        .path
                        .extension()
                        .map(|e| e.to_string_lossy().to_uppercase())
                        .unwrap_or_else(|| "FILE".into()))
                )
            };
            let caption = if item.caption.is_empty() { &name } else { &item.caption };
            format!(
                "<figure>{media}<figcaption><h3>{caption}</h3>{desc}<span class=path>{name}</span></figcaption></figure>",
                caption = esc(caption),
                desc = (!item.description.is_empty())
                    .then(|| format!("<p>{}</p>", esc(&item.description)))
                    .unwrap_or_default(),
                name = esc(&name),
            )
        })
        .collect();
    let body = format!(
        "<header><div><h1>{title}</h1><div class=meta>{from} · {time} · {n} item{s}</div></div>\
         <a class=all href=\"../\">All results</a></header>\
         <main>{desc}<div class=gallery>{items}</div></main>",
        title = esc(&s.title),
        from = esc(&s.from_name),
        time = esc(&s.time),
        n = s.items.len(),
        s = plural(s.items.len()),
        desc = (!s.description.is_empty())
            .then(|| format!("<p class=question>{}</p>", esc(&s.description)))
            .unwrap_or_default(),
    );
    document(app, &s.title, SHOW_CSS, &body)
}

const SHOW_CSS: &str = "<style>
.all{color:var(--subtext);text-decoration:none;font-size:.82rem;border:1px solid color-mix(in srgb,var(--text) 12%,transparent);border-radius:8px;padding:.35rem .9rem}
.all:hover{color:var(--text);border-color:var(--accent)}
.gallery{display:grid;grid-template-columns:repeat(auto-fill,minmax(340px,1fr));gap:1.1rem}
figure{margin:0;background:var(--mantle);border:1.5px solid color-mix(in srgb,var(--text) 9%,transparent);border-radius:12px;overflow:hidden;display:flex;flex-direction:column}
.media{display:flex;align-items:center;justify-content:center;background:var(--crust);aspect-ratio:4/3;max-height:60vh}
.media img,.media video{width:100%;height:100%;object-fit:contain}
a.media:hover{outline:2px solid var(--accent);outline-offset:-2px}
.media.audio{aspect-ratio:auto;padding:1.4rem}
.media.audio audio{width:100%}
.media.file{text-decoration:none;aspect-ratio:auto;padding:2.2rem}
.media.file span{font-weight:600;letter-spacing:.06em;color:var(--accent);border:1.5px solid var(--accent);border-radius:8px;padding:.6rem 1rem}
figcaption{padding:.8rem 1rem;display:flex;flex-direction:column;gap:.2rem}
figcaption p{font-size:.85rem;color:var(--subtext)}
.path{font-size:.72rem;color:var(--overlay);font-family:ui-monospace,monospace;word-break:break-all}
.sets{display:flex;flex-direction:column;gap:.7rem;max-width:860px}
.set{display:flex;gap:1rem;align-items:center;text-decoration:none;color:var(--text);background:var(--mantle);border:1.5px solid color-mix(in srgb,var(--text) 9%,transparent);border-radius:12px;padding:.6rem}
.set:hover{border-color:var(--accent)}
.set p{font-size:.8rem;color:var(--overlay)}
.thumb{width:96px;height:72px;border-radius:8px;background:var(--crust);overflow:hidden;flex-shrink:0}
.thumb img{width:100%;height:100%;object-fit:cover}
</style>";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_checks_files() {
        let file = std::env::temp_dir().join("termden-showcase-test.png");
        std::fs::write(&file, b"x").unwrap();
        let ok = parse(&json!({"title": "Logos", "items": [{"path": file, "caption": "A"}]}));
        assert_eq!(ok.unwrap().2.len(), 1);
        assert!(parse(&json!({"items": [{"path": file}]})).is_err(), "title");
        assert!(parse(&json!({"title": "x", "items": []})).is_err(), "empty");
        let err = parse(&json!({"title": "x", "items": [{"path": "relative.png"}]}));
        assert!(err.unwrap_err().contains("relative.png"));
        std::fs::remove_file(file).ok();
    }
}
