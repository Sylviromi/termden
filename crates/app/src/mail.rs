//! Mailboxes: agents message each other through termden instead of typing
//! into each other's prompts. The text is pulled with `termden_read_mail`
//! (MCP) or `termdenctl mail read`; an idle agent only gets a one-line nudge.

use std::time::{Duration, Instant};

use gpui::{Context, Timer};
use termden_core::{HarnessId, HarnessKind, HarnessStatus, Mail};

use crate::state::{Mode, TermdenApp};

const KEEP_READ: usize = 100;
/// How long the human must have been quiet before a nudge may be typed.
const HUMAN_QUIET: Duration = Duration::from_secs(5);

impl TermdenApp {
    pub fn send_mail(&mut self, from: Option<HarnessId>, to: HarnessId, text: String) {
        let sender_color = from
            .and_then(|id| self.harness(id))
            .map(|(w, h)| h.color.unwrap_or(w.accent));
        let from = from.and_then(|id| self.harness(id)).map_or_else(
            || "user".to_string(),
            |(_, h)| match h.tag.as_str() {
                "" => h.name.clone(),
                tag => format!("{} [{tag}]", h.name),
            },
        );
        if let Some((workspace, target)) = self.harness(to) {
            let target = (
                target.name.clone(),
                target.color.unwrap_or(workspace.accent),
            );
            let sender = from.split(" [").next().unwrap_or(&from).to_string();
            self.log(
                sender,
                "sent a message".into(),
                sender_color.unwrap_or(usize::MAX),
            );
            if let Some(entry) = self.activity.last_mut() {
                entry.to = Some(target);
            }
        }
        self.mail.push(Mail {
            to,
            from,
            text,
            read: false,
        });
        let mut excess = self
            .mail
            .iter()
            .filter(|m| m.read)
            .count()
            .saturating_sub(KEEP_READ);
        self.mail.retain(|m| {
            let drop = m.read && excess > 0;
            excess -= drop as usize;
            !drop
        });
        self.sync_unread();
        self.save();
    }

    /// Unread mail for `id`, marked read.
    pub fn read_mail(&mut self, id: HarnessId) -> Vec<Mail> {
        let mut out = Vec::new();
        for mail in self.mail.iter_mut().filter(|m| m.to == id && !m.read) {
            mail.read = true;
            out.push(mail.clone());
        }
        self.sync_unread();
        self.save();
        out
    }

    pub fn sync_unread(&mut self) {
        for workspace in &mut self.workspaces {
            for harness in &mut workspace.harnesses {
                harness.unread = self
                    .mail
                    .iter()
                    .filter(|m| m.to == harness.id && !m.read)
                    .count() as u16;
            }
        }
    }

    /// Type a one-line nudge into agents that are idle and have unread mail.
    /// Shells are skipped (typing there would run a command).
    pub fn notify_mail(&mut self, cx: &mut Context<Self>) {
        if self.mode != Mode::Live {
            return;
        }
        let pending: Vec<(HarnessId, u16)> = self
            .workspaces
            .iter()
            .flat_map(|w| &w.harnesses)
            .filter(|h| h.unread > 0)
            .map(|h| (h.id, h.unread))
            .collect();
        self.mail_notified
            .retain(|id, _| pending.iter().any(|(p, _)| p == id));
        for (id, unread) in pending {
            let Some((_, harness)) = self.harness(id) else {
                continue;
            };
            if harness.kind == HarnessKind::Shell
                || harness.status != HarnessStatus::Idle
                || self.mail_notified.get(&id).is_some_and(|n| *n >= unread)
            {
                continue;
            }
            let Some(session) = self.sessions.get_mut(&id).filter(|s| !s.exited) else {
                continue;
            };
            // Never type over the human. A nudge lands in the agent's prompt,
            // so it would be merged with anything the user is composing there
            // and our Enter would submit their half-written message. Wait for
            // a prompt that is quiet, empty, and has no unsent draft.
            if session.last_human.elapsed() < HUMAN_QUIET
                || session.draft > 0
                || !session.prompt_is_empty()
            {
                continue;
            }
            let Some(nudge) = nudge(&self.mail, id) else {
                continue;
            };
            let Some(session) = self.sessions.get_mut(&id) else {
                continue;
            };
            session.inject(nudge.into_bytes());
            self.mail_notified.insert(id, unread);
            let sent = Instant::now();
            // Separate keystroke so TUIs submit instead of treating it as a paste.
            cx.spawn(async move |this, cx| {
                Timer::after(Duration::from_millis(150)).await;
                let _ = this.update(cx, |this, _| {
                    let Some(session) = this.sessions.get_mut(&id) else {
                        return;
                    };
                    // The human started typing in the gap: leave the nudge for
                    // them to see, but do not submit whatever is in the prompt.
                    if session.last_human > sent {
                        return;
                    }
                    session.inject(&b"\r"[..]);
                    session.draft = 0;
                });
            })
            .detach();
        }
    }
}

/// "📬 sender: first words of the newest unread mail… (+N more)". Control
/// characters are dropped: mail text is typed into a terminal.
fn nudge(mail: &[Mail], to: HarnessId) -> Option<String> {
    let unread: Vec<&Mail> = mail.iter().filter(|m| m.to == to && !m.read).collect();
    let last = unread.last()?;
    let text: String = last
        .text
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let mut preview: String = text.chars().take(60).collect();
    if text.chars().count() > 60 {
        preview.push('…');
    }
    let from: String = last.from.chars().filter(|c| !c.is_control()).collect();
    let more = match unread.len() {
        1 => String::new(),
        n => format!(" (+{} more)", n - 1),
    };
    Some(format!("📬 {from}: {preview}{more}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudge_previews_newest_unread() {
        let to = HarnessId::new(7);
        let m = |text: &str, read| Mail {
            to,
            from: "user".into(),
            text: text.into(),
            read,
        };
        assert_eq!(nudge(&[m("old", true)], to), None);
        assert_eq!(
            nudge(&[m("a", false), m("line\none\x1b[2J", false)], to).as_deref(),
            Some("📬 user: line one[2J (+1 more)")
        );
        let long = "x".repeat(70);
        assert_eq!(
            nudge(&[m(&long, false)], to),
            Some(format!("📬 user: {}…", "x".repeat(60)))
        );
    }
}
