//! Self-update from GitHub releases. A few times a day the app asks for the
//! latest release tag; when it is newer, the footer offers it. Installing
//! swaps `termden` and `termdenctl` next to the running binary, and the
//! existing rebuild watcher (`check_rebuilt`) execs the new one, so every
//! harness survives the update.
//!
//! shortcut: shells out to `curl` and `tar` (the installer needs them anyway)
//! instead of pulling an HTTP client into the build.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use gpui::{Context, Timer};

use crate::state::TermdenApp;

pub const REPO: &str = "Sylviromi/termden";
const ASSET: &str = "termden-x86_64-linux.tar.gz";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

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

/// Latest release tag, e.g. `v0.2.0`.
fn latest() -> Option<String> {
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "20"])
        .arg(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .output()
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    json["tag_name"].as_str().map(String::from)
}

fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.');
    let mut next = || parts.next()?.split('-').next()?.parse().ok();
    Some((next()?, next()?, next()?))
}

fn newer(tag: &str, current: &str) -> bool {
    matches!((parse(tag), parse(current)), (Some(a), Some(b)) if a > b)
}

/// Download `tag` and swap the binaries in `dir`; termden goes last because
/// replacing it triggers the reload.
fn install(tag: &str, dir: &Path) -> Result<(), String> {
    let tmp = dir.join(".termden-update");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let result = (|| {
        let archive = tmp.join(ASSET);
        let run = |cmd: &mut Command| match cmd.status() {
            Ok(s) if s.success() => Ok(()),
            _ => Err(format!("{:?} failed", cmd.get_program())),
        };
        run(Command::new("curl")
            .args(["-fsSL", "--max-time", "300", "-o"])
            .arg(&archive)
            .arg(format!(
                "https://github.com/{REPO}/releases/download/{tag}/{ASSET}"
            )))?;
        run(Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&tmp))?;
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
                let tag = cx.background_executor().spawn(async { latest() }).await;
                let alive = this.update(cx, |this, cx| {
                    this.update_available = tag.filter(|t| newer(t, VERSION));
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

    /// Install the offered release; the binary watcher reloads into it.
    pub fn apply_update(&mut self, cx: &mut Context<Self>) -> String {
        let (Some(tag), Some(dir)) = (self.update_available.clone(), install_dir()) else {
            return format!("termden {VERSION} is up to date");
        };
        if self.updating {
            return format!("already updating to {tag}");
        }
        self.updating = true;
        self.log("Termden", format!("updating to {tag}"), 0);
        let reply = format!("updating to {tag}");
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { install(&tag, &dir) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.updating = false;
                match result {
                    // The user clicked Update: restart into it right away.
                    Ok(()) => this.reload(),
                    Err(error) => this.log("Termden", format!("update failed: {error}"), 7),
                }
                cx.notify();
            });
        })
        .detach();
        reply
    }
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
}
