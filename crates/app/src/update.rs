//! Self-update from GitHub releases. A few times a day the app asks for the
//! latest release; when it is newer, a download button appears next to
//! settings. Its popup shows the release notes; Update downloads with a
//! progress bar and swaps `termden` and `termdenctl` next to the running
//! binary; then the user confirms the restart into it (harnesses survive).
//!
//! shortcut: shells out to `curl` and `tar` (the installer needs them anyway)
//! instead of pulling an HTTP client into the build.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gpui::{AnyElement, Context, FontWeight, Timer, div, prelude::*, px};

use crate::state::TermdenApp;
use crate::widgets::{icon_button, icons};

pub const REPO: &str = "Sylviromi/termden";
const ASSET: &str = "termden-x86_64-linux.tar.gz";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

/// A newer release offered to the user.
#[derive(Clone)]
pub struct Release {
    pub tag: String,
    pub notes: String,
    url: String,
    size: u64,
}

/// Where an update stands; drives the header button and the popup.
pub enum Phase {
    Offered,
    /// Bytes downloaded so far, written by the download thread.
    Downloading(Arc<AtomicU64>),
    Installed,
    Failed(String),
}

pub struct Update {
    pub release: Release,
    pub phase: Phase,
}

/// Directory of an installed (not `cargo build`) binary we may replace.
fn install_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if exe.components().any(|c| c.as_os_str() == "target") {
        return None; // dev build: updates come from the checkout
    }
    let dir = exe.parent()?.to_path_buf();
    let probe = dir.join(".termden-write-test");
    std::fs::write(&probe, b"").ok()?;
    let _ = std::fs::remove_file(probe);
    Some(dir)
}

/// The latest release: tag, notes, and our tarball's URL and size. The
/// release-info URL can be overridden (`TERMDEN_UPDATE_URL`) to test the
/// whole flow against a local `file://` JSON.
fn latest() -> Option<Release> {
    let api = std::env::var("TERMDEN_UPDATE_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "20"])
        .arg(api)
        .output()
        .ok()?;
    parse_release(&serde_json::from_slice(&out.stdout).ok()?)
}

fn parse_release(json: &serde_json::Value) -> Option<Release> {
    let tag = json["tag_name"].as_str()?.to_string();
    let asset = json["assets"]
        .as_array()?
        .iter()
        .find(|a| a["name"] == ASSET)?;
    Some(Release {
        url: asset["browser_download_url"].as_str()?.to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
        notes: json["body"].as_str().unwrap_or("").trim().to_string(),
        tag,
    })
}

fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.');
    let mut next = || parts.next()?.split('-').next()?.parse().ok();
    Some((next()?, next()?, next()?))
}

fn newer(tag: &str, current: &str) -> bool {
    matches!((parse(tag), parse(current)), (Some(a), Some(b)) if a > b)
}

/// Download `release` (counting bytes into `done`) and swap the binaries in
/// `dir`. The running app keeps its old binary until the user restarts.
fn install(release: &Release, dir: &Path, done: &AtomicU64) -> Result<(), String> {
    let tmp = dir.join(".termden-update");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let result = (|| {
        let archive = tmp.join(ASSET);
        let mut curl = Command::new("curl")
            .args(["-fsSL", "--max-time", "600", "-o"])
            .arg(&archive)
            .arg(&release.url)
            .spawn()
            .map_err(|e| format!("curl: {e}"))?;
        let status = loop {
            if let Some(status) = curl.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            let have = std::fs::metadata(&archive).map_or(0, |m| m.len());
            done.store(have, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(100));
        };
        if !status.success() {
            return Err("download failed".into());
        }
        done.store(release.size.max(1), Ordering::Relaxed);
        let unpacked = Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&tmp)
            .status();
        if !unpacked.is_ok_and(|s| s.success()) {
            return Err("could not unpack the release".into());
        }
        for bin in ["termdenctl", "termden"] {
            let new = tmp.join("termden").join(bin);
            if !new.is_file() {
                return Err(format!("{bin} missing from the release"));
            }
            std::fs::rename(&new, dir.join(bin)).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

impl TermdenApp {
    /// Check for a newer release now and every few hours.
    pub fn watch_updates(cx: &mut Context<Self>) {
        if install_dir().is_none() {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                let release = cx.background_executor().spawn(async { latest() }).await;
                let alive = this.update(cx, |this, cx| {
                    let busy = this
                        .update
                        .as_ref()
                        .is_some_and(|u| !matches!(u.phase, Phase::Offered));
                    if !busy {
                        this.update =
                            release
                                .filter(|r| newer(&r.tag, VERSION))
                                .map(|release| Update {
                                    release,
                                    phase: Phase::Offered,
                                });
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
                Timer::after(CHECK_EVERY).await;
            }
        })
        .detach();
    }

    /// Download and install the offered release; the popup follows along.
    pub fn apply_update(&mut self, cx: &mut Context<Self>) -> String {
        let Some(dir) = install_dir() else {
            return format!("termden {VERSION}: source builds update by rebuilding");
        };
        let Some(update) = self.update.as_mut() else {
            return format!("termden {VERSION} is up to date");
        };
        if !matches!(update.phase, Phase::Offered | Phase::Failed(_)) {
            return format!("already updating to {}", update.release.tag);
        }
        let done = Arc::new(AtomicU64::new(0));
        update.phase = Phase::Downloading(done.clone());
        let release = update.release.clone();
        let reply = format!("updating to {}", release.tag);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { install(&release, &dir, &done) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(update) = this.update.as_mut() {
                    update.phase = match result {
                        Ok(()) => Phase::Installed,
                        Err(error) => Phase::Failed(error),
                    };
                }
                cx.notify();
            });
        })
        .detach();
        reply
    }

    /// Redraw on the frame tick while the progress bar moves.
    pub fn update_downloading(&self) -> bool {
        self.update
            .as_ref()
            .is_some_and(|u| matches!(u.phase, Phase::Downloading(_)))
    }
}

/// Header button next to settings, shown while an update is on offer.
pub fn button(app: &TermdenApp, size: f32, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    app.update.as_ref()?;
    let theme = &app.theme;
    Some(
        div()
            .relative()
            .child(
                icon_button("update", icons::DOWNLOAD, theme)
                    .size(px(size))
                    .text_size(theme.ui(15.))
                    .text_color(theme.green)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_open = true;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(3.))
                    .size(px(6.))
                    .rounded(px(3.))
                    .bg(theme.green),
            )
            .into_any_element(),
    )
}

/// The update popup: notes and Later/Update, then a progress bar, then the
/// restart confirmation.
pub fn render(app: &TermdenApp, cx: &mut Context<TermdenApp>) -> Option<AnyElement> {
    let closing = app.popups[crate::popup::UPDATE].closing();
    if !app.update_open && !closing {
        return None;
    }
    let update = app.update.as_ref()?;
    let theme = &app.theme;
    let tag = &update.release.tag;
    let button = |id: &'static str, label: &'static str, primary: bool| {
        div()
            .id(id)
            .px(px(14.))
            .h(px(30.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .text_size(theme.ui(11.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .map(|d| {
                if primary {
                    d.bg(theme.green).text_color(theme.crust)
                } else {
                    d.border_1()
                        .border_color(theme.text.opacity(0.12))
                        .text_color(theme.subtext)
                        .hover(|s| s.bg(theme.surface0))
                }
            })
            .child(label)
    };
    let later = button("update-later", "Later", false).on_click(cx.listener(|this, _, _, cx| {
        this.update_open = false;
        cx.notify();
    }));
    let (title, body, actions): (String, AnyElement, Vec<AnyElement>) = match &update.phase {
        Phase::Offered | Phase::Failed(_) => {
            let failed = match &update.phase {
                Phase::Failed(error) => Some(error.clone()),
                _ => None,
            };
            (
                "Update available".into(),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(notes(app, &update.release.notes))
                    .children(failed.map(|error| {
                        div()
                            .text_size(theme.ui(10.5))
                            .text_color(theme.red)
                            .child(format!("Update failed: {error}. Try again?"))
                    }))
                    .into_any_element(),
                vec![
                    later.into_any_element(),
                    button("update-now", "Update", true)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.apply_update(cx);
                            cx.notify();
                        }))
                        .into_any_element(),
                ],
            )
        }
        Phase::Downloading(done) => {
            let total = update.release.size.max(1);
            let have = done.load(Ordering::Relaxed).min(total);
            let fraction = have as f32 / total as f32;
            let mb = |b: u64| b as f32 / 1_048_576.0;
            (
                format!("Updating to {tag}"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .h(px(8.))
                            .w_full()
                            .rounded(px(4.))
                            .bg(theme.surface0)
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(fraction))
                                    .rounded(px(4.))
                                    .bg(theme.green),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.ui(10.))
                            .text_color(theme.overlay)
                            .child(format!(
                                "{:.1} / {:.1} MB · {:.0}%",
                                mb(have),
                                mb(total),
                                fraction * 100.0
                            )),
                    )
                    .into_any_element(),
                Vec::new(),
            )
        }
        Phase::Installed => (
            format!("{tag} is installed"),
            div()
                .text_size(theme.ui(11.))
                .text_color(theme.subtext)
                .child("Restart termden to switch to it. Your tiles keep running.")
                .into_any_element(),
            vec![
                later.into_any_element(),
                button("update-restart", "Restart now", true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_open = false;
                        this.reload();
                        cx.notify();
                    }))
                    .into_any_element(),
            ],
        ),
    };
    let width = 440.0;
    let card = div()
        .id("update-card")
        .w(px(width))
        .flex()
        .flex_col()
        .rounded(px(14.))
        .bg(theme.mantle)
        .border_1()
        .border_color(theme.text.opacity(0.10))
        .shadow_lg()
        .overflow_hidden()
        .child(crate::popup::drag_handle(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .px(px(18.))
                .py(px(14.))
                .border_b_1()
                .border_color(theme.text.opacity(0.06))
                .child(
                    div()
                        .text_size(theme.ui(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(theme.ui(10.))
                        .text_color(theme.overlay)
                        .child(format!("termden v{VERSION} → {tag}")),
                ),
            crate::popup::UPDATE,
            (width, 420.0),
            cx,
        ))
        .child(div().p(px(18.)).child(body))
        .children((!actions.is_empty()).then(|| {
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .px(px(18.))
                .pb(px(16.))
                .children(actions)
        }));
    Some(crate::popup::frame(
        "update-backdrop",
        crate::popup::UPDATE,
        app,
        card,
        |this| this.update_open = false,
        cx,
    ))
}

/// Release notes: `- item` lines as bullets, `## heading` lines as headings.
fn notes(app: &TermdenApp, text: &str) -> AnyElement {
    let theme = &app.theme;
    let lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    div()
        .id("update-notes")
        .max_h(px(260.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(5.))
        .children(lines.map(|line| {
            if let Some(item) = line.strip_prefix("- ").or(line.strip_prefix("* ")) {
                div()
                    .flex()
                    .gap(px(8.))
                    .text_size(theme.ui(11.))
                    .text_color(theme.text)
                    .child(div().text_color(theme.green).child("•"))
                    .child(div().flex_1().min_w(px(0.)).child(item.to_string()))
            } else if let Some(heading) = line.strip_prefix('#').map(|h| h.trim_start_matches('#'))
            {
                div()
                    .pt(px(4.))
                    .text_size(theme.ui(9.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.overlay)
                    .child(heading.trim().to_uppercase())
            } else {
                div()
                    .text_size(theme.ui(10.5))
                    .text_color(theme.subtext)
                    .child(line.to_string())
            }
        }))
        .when(text.trim().is_empty(), |d| {
            d.child(
                div()
                    .text_size(theme.ui(10.5))
                    .text_color(theme.subtext)
                    .child("A new version of termden is ready."),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(newer("v1.0.0", "0.10.0"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("v0.1.0-rc1", "0.1.0"));
        assert!(!newer("nightly", "0.1.0"));
    }

    #[test]
    fn reads_release_json() {
        let json = serde_json::json!({
            "tag_name": "v0.2.0",
            "body": "## Changes\n- Themes\n",
            "assets": [
                {"name": "other.tar.gz", "browser_download_url": "x", "size": 1},
                {"name": ASSET, "browser_download_url": "https://e/t.tgz", "size": 12345}
            ]
        });
        let r = parse_release(&json).unwrap();
        assert_eq!(
            (r.tag.as_str(), r.url.as_str(), r.size),
            ("v0.2.0", "https://e/t.tgz", 12345)
        );
        assert!(r.notes.contains("Themes"));
        assert!(parse_release(&serde_json::json!({"tag_name": "v1", "assets": []})).is_none());
    }
}
