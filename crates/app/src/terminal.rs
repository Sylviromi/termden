use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{AnyElement, FontWeight, Hsla, div, prelude::*, px};
use termden_core::{Harness, HarnessKind, HarnessStatus};

use crate::pty::Session;
use crate::theme::Theme;
use crate::widgets::spinner_frame;

struct Span {
    text: String,
    color: Hsla,
    bg: Option<Hsla>,
    bold: bool,
    /// Grid cells covered; 0 = size to the text (mock buffers).
    cells: usize,
}

struct Line {
    spans: Vec<Span>,
}

#[derive(Default)]
struct Buf {
    lines: Vec<Line>,
    current: Line,
}

impl Buf {
    fn text(&mut self, text: impl Into<String>, color: Hsla) -> &mut Self {
        self.current.spans.push(Span {
            cells: 0,
            text: text.into(),
            color,
            bg: None,
            bold: false,
        });
        self
    }

    fn bold(&mut self, text: impl Into<String>, color: Hsla) -> &mut Self {
        self.current.spans.push(Span {
            cells: 0,
            text: text.into(),
            color,
            bg: None,
            bold: true,
        });
        self
    }

    fn nl(&mut self) -> &mut Self {
        let line = std::mem::take(&mut self.current);
        self.lines.push(line);
        self
    }

    fn blank(&mut self) -> &mut Self {
        self.nl()
    }

    fn finish(mut self) -> Vec<Line> {
        if !self.current.spans.is_empty() {
            self.nl();
        }
        self.lines
    }
}

impl Default for Line {
    fn default() -> Self {
        Self { spans: Vec::new() }
    }
}

pub fn render(harness: &Harness, workspace_path: &str, theme: &Theme, tick: u64) -> AnyElement {
    lines_element(build(harness, workspace_path, theme, tick), theme, 0.0)
}

/// Draw a live terminal grid. Cells with identical style merge into spans;
/// each span is pinned to `cells * cell_w` so fallback-font glyphs (block
/// characters, emoji) cannot shift the rest of the row.
pub fn render_live(
    session: &Session,
    theme: &Theme,
    tick: u64,
    focused: bool,
    cell_w: f32,
) -> AnyElement {
    let term = session.term.lock();
    let content = term.renderable_content();
    let offset = content.display_offset as i32;
    let cursor = content.cursor;
    let show_cursor =
        focused && !session.exited && cursor.shape != CursorShape::Hidden && (tick / 4) % 2 == 0;

    let mut lines: Vec<Line> = (0..term.screen_lines()).map(|_| Line::default()).collect();
    for indexed in content.display_iter {
        let cell = indexed.cell;
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let Some(line) = lines.get_mut((indexed.point.line.0 + offset) as usize) else {
            continue;
        };

        let mut fg = theme.term_color(cell.fg).unwrap_or(theme.text);
        let mut bg = theme.term_color(cell.bg);
        if cell.flags.contains(Flags::INVERSE) {
            (fg, bg) = (bg.unwrap_or(theme.base), Some(fg));
        }
        if cell.flags.contains(Flags::DIM) {
            fg = fg.opacity(0.7);
        }
        if show_cursor && indexed.point == cursor.point {
            (fg, bg) = (theme.base, Some(theme.text));
        }
        let bold = cell.flags.contains(Flags::BOLD);
        let ch = if cell.c == ' ' || cell.flags.contains(Flags::HIDDEN) {
            '\u{00A0}'
        } else {
            cell.c
        };

        let width = if cell.flags.contains(Flags::WIDE_CHAR) {
            2
        } else {
            1
        };
        match line.spans.last_mut() {
            Some(span) if span.color == fg && span.bg == bg && span.bold == bold => {
                span.text.push(ch);
                span.cells += width;
            }
            _ => line.spans.push(Span {
                text: ch.to_string(),
                color: fg,
                bg,
                bold,
                cells: width,
            }),
        }
    }

    lines_element(lines, theme, cell_w)
}

fn lines_element(lines: Vec<Line>, theme: &Theme, cell_w: f32) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .justify_end()
        .size_full()
        .overflow_hidden()
        .px(px(10.))
        .py(px(8.))
        .font_family(theme.mono_font.clone())
        .text_size(px(theme.term_font))
        .line_height(px(theme.term_line))
        .children(lines.into_iter().map(|line| {
            div()
                .flex()
                .flex_row()
                .h(px(theme.term_line))
                .flex_shrink_0()
                .whitespace_nowrap()
                .children(line.spans.into_iter().map(|span| {
                    let mut element = div()
                        .text_color(span.color)
                        .child(span.text.replace(' ', "\u{00A0}"));
                    if span.cells > 0 {
                        element = element
                            .w(px(cell_w * span.cells as f32))
                            .flex_shrink_0()
                            .overflow_hidden();
                    }
                    if let Some(bg) = span.bg {
                        element = element.bg(bg);
                    }
                    if span.bold {
                        element = element.font_weight(FontWeight::SEMIBOLD);
                    }
                    element
                }))
        }))
        .into_any_element()
}

fn build(harness: &Harness, workspace_path: &str, theme: &Theme, tick: u64) -> Vec<Line> {
    let mut buf = Buf::default();
    match harness.kind {
        HarnessKind::ClaudeCode => claude(&mut buf, harness, workspace_path, theme, tick),
        HarnessKind::OpenCode => opencode(&mut buf, harness, workspace_path, theme, tick),
        HarnessKind::Pi => pi(&mut buf, harness, workspace_path, theme, tick),
        HarnessKind::Shell => shell(&mut buf, harness, workspace_path, theme, tick),
    }
    buf.finish()
}

fn claude(buf: &mut Buf, harness: &Harness, root: &str, theme: &Theme, tick: u64) {
    buf.bold("✻ ", theme.mauve)
        .bold("Claude Code", theme.text)
        .text("  v2.1.7", theme.overlay)
        .nl();
    buf.text(format!("  {root}"), theme.overlay).nl();
    buf.blank();

    buf.text("⏺ ", theme.green)
        .text("Read(crates/app/src/canvas.rs)", theme.text)
        .nl();
    buf.text("  ⎿ ", theme.overlay)
        .text("Read 268 lines", theme.subtext)
        .nl();
    buf.blank();

    buf.text("⏺ ", theme.mauve)
        .text("Task(explore dwindle layout)", theme.text)
        .nl();
    let running = harness.working_subagents();
    buf.text("  ⎿ ", theme.overlay)
        .text(
            format!(
                "{running} subagent{} running",
                if running == 1 { "" } else { "s" }
            ),
            theme.green,
        )
        .nl();

    for (index, agent) in harness.subagents.iter().take(2).enumerate() {
        let last = index + 1 == harness.subagents.len().min(2);
        buf.text(
            if last { "    └─ " } else { "    ├─ " },
            theme.text.opacity(0.20),
        )
        .text(agent.label.clone(), theme.subtext)
        .text("  ", theme.overlay);
        if agent.working {
            buf.text(spinner_frame(tick), theme.green);
        } else {
            buf.text("✓", theme.overlay);
        }
        buf.text(format!(" {}", agent.detail), theme.overlay).nl();
    }
    buf.blank();

    buf.text("⏺ ", theme.peach)
        .text("Glob(**/*.rs)", theme.text)
        .nl();
    buf.text("  ⎿ ", theme.overlay)
        .text("Found 41 files", theme.subtext)
        .nl();
    buf.blank();

    buf.text("⏺ ", theme.green)
        .text("Read(crates/core/src/layout.rs)", theme.text)
        .nl();
    buf.text("  ⎿ ", theme.overlay)
        .text("Read 214 lines", theme.subtext)
        .nl();
    buf.blank();

    buf.text("> ", theme.blue)
        .text("make the gap configurable", theme.text)
        .nl();
    buf.text("⏺ ", theme.peach)
        .text("Edit(crates/app/src/canvas.rs)", theme.text)
        .nl();
    buf.text("  ⎿ ", theme.overlay)
        .text("Updated config.rs · +9 −0", theme.subtext)
        .nl();
    buf.blank();

    buf.text("⏺ ", theme.green)
        .text("Bash(cargo check --workspace)", theme.text)
        .nl();
    buf.text("  ⎿ ", theme.overlay)
        .text("Finished `dev` profile in 0.42s", theme.subtext)
        .nl();
    buf.blank();

    buf.text("  ⎿ ", theme.overlay)
        .text("Todo", theme.subtext)
        .nl();
    buf.text("    ☒ ", theme.overlay)
        .text("thread gap through config", theme.overlay)
        .nl();
    buf.text("    ☐ ", theme.overlay)
        .text("add the everforest palette", theme.subtext)
        .nl();
    buf.blank();

    status_footer(buf, harness.status, theme, tick);
    buf.text("  1.2k tokens · 18% context", theme.overlay.opacity(0.85))
        .nl();
}

fn opencode(buf: &mut Buf, harness: &Harness, root: &str, theme: &Theme, tick: u64) {
    buf.bold("opencode", theme.green)
        .text("  0.6.2", theme.overlay)
        .text("  ses_9f2c", theme.overlay)
        .nl();
    buf.text("─".repeat(46), theme.text.opacity(0.14)).nl();
    buf.text("> ", theme.blue)
        .text("add a subagent monitor to the panel", theme.text)
        .nl();
    buf.blank();

    buf.text("● ", theme.yellow)
        .bold("read", theme.text)
        .text("   crates/core/src/model.rs", theme.overlay)
        .nl();
    buf.text("● ", theme.yellow)
        .bold("grep", theme.text)
        .text("   \"Subagent\" · 12 hits", theme.overlay)
        .nl();
    buf.text("● ", theme.yellow)
        .bold("task", theme.text)
        .text("   update subagent panel", theme.overlay)
        .nl();
    buf.text("● ", theme.yellow)
        .bold("write", theme.text)
        .text("  crates/app/src/panel.rs", theme.overlay)
        .nl();
    buf.blank();

    buf.text(spinner_frame(tick), theme.green)
        .text(" build", theme.text)
        .text(
            format!("   {} agents", harness.subagent_count().max(1)),
            theme.overlay,
        )
        .nl();
    for (index, agent) in harness.subagents.iter().take(2).enumerate() {
        let last = index + 1 == harness.subagents.len().min(2);
        buf.text(
            if last { "  └─ " } else { "  ├─ " },
            theme.text.opacity(0.20),
        )
        .text(agent.label.clone(), theme.subtext)
        .text(format!(" · {}", agent.detail), theme.overlay)
        .nl();
    }
    buf.blank();
    buf.text(format!("  cwd {root}"), theme.overlay.opacity(0.85))
        .nl();
    match harness.status {
        HarnessStatus::NeedsInput => {
            buf.text("? ", theme.blue)
                .text("Waiting for input", theme.blue)
                .nl();
        }
        HarnessStatus::Error => {
            buf.text("✗ ", theme.red)
                .text("provider error · retry scheduled", theme.red)
                .nl();
        }
        _ => {
            buf.text("18.4k tokens · $0.42", theme.overlay.opacity(0.85))
                .nl();
        }
    }
}

fn pi(buf: &mut Buf, harness: &Harness, root: &str, theme: &Theme, tick: u64) {
    buf.bold("π ", theme.mauve)
        .bold("pi", theme.text)
        .text("  0.4.0", theme.overlay)
        .nl();
    buf.text(format!("  {root}"), theme.overlay).nl();
    buf.blank();
    buf.text("> ", theme.blue)
        .text("port the drop zone hit test", theme.text)
        .nl();
    buf.text("● ", theme.green)
        .text("bash", theme.text)
        .text("  cargo check --workspace", theme.overlay)
        .nl();
    buf.text("  ⎿ ", theme.overlay)
        .text("finished in 4.2s", theme.subtext)
        .nl();
    buf.blank();
    status_footer(buf, harness.status, theme, tick);
}

fn shell(buf: &mut Buf, _harness: &Harness, root: &str, theme: &Theme, tick: u64) {
    buf.bold("termden", theme.green)
        .text("  main", theme.blue)
        .text("  via ⚡", theme.overlay)
        .nl();
    buf.text(format!("  {root}"), theme.overlay).nl();
    buf.blank();
    buf.text("$ ", theme.green)
        .text("cargo check --workspace", theme.text)
        .nl();
    buf.text("    Checking termden-core v0.1.0", theme.overlay)
        .nl();
    buf.text("    Checking termden-app v0.1.0", theme.overlay)
        .nl();
    buf.text("    Finished `dev` profile in 4.21s", theme.green)
        .nl();
    buf.blank();
    buf.text("$ ", theme.green)
        .text("git status --short", theme.text)
        .nl();
    buf.text(" M ", theme.yellow)
        .text("crates/app/src/canvas.rs", theme.subtext)
        .nl();
    buf.text("?? ", theme.red)
        .text("crates/app/src/theme.rs", theme.subtext)
        .nl();
    buf.blank();
    buf.text("$ ", theme.green)
        .text(
            if (tick / 4) % 2 == 0 {
                "▊"
            } else {
                "\u{00A0}"
            },
            theme.green,
        )
        .nl();
}

fn status_footer(buf: &mut Buf, status: HarnessStatus, theme: &Theme, tick: u64) {
    match status {
        HarnessStatus::Working => {
            buf.text("✻ ", theme.mauve)
                .text(spinner_frame(tick), theme.mauve)
                .text(" Thinking…", theme.overlay)
                .text("  (esc to interrupt)", theme.overlay.opacity(0.7))
                .nl();
        }
        HarnessStatus::NeedsInput => {
            buf.text("? ", theme.blue)
                .text("Waiting for input", theme.blue)
                .nl();
        }
        HarnessStatus::Idle => {
            buf.text("✓ ", theme.overlay)
                .text("Idle", theme.overlay)
                .nl();
        }
        HarnessStatus::Error => {
            buf.text("✗ ", theme.red)
                .text("429 Too Many Requests", theme.red)
                .nl();
        }
    }
}
