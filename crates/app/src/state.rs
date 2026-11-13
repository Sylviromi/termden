use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::term::TermMode;
use gpui::{
    AnyElement, ClipboardItem, Context, FocusHandle, KeyDownEvent, MouseButton, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, ScrollHandle, ScrollWheelEvent, Timer, Window, div, font,
    prelude::*, px,
};
use termden_core::{
    AppState, Config, DropZone, Harness, HarnessId, HarnessKind, HarnessStatus, LayoutNode,
    Workspace, WorkspaceId,
};

use crate::actions;
use crate::canvas::{self, TRect};
use crate::control;
use crate::launch;
use crate::mock;
use crate::panel;
use crate::pty::{self, Session};
use crate::theme::Theme;
use crate::tile;
use crate::widgets::{dot, hspacer, kind_badge};

#[derive(Clone, Copy)]
pub struct DragState {
    pub harness: HarnessId,
    pub start: Point<Pixels>,
    pub pos: Point<Pixels>,
    pub active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Mock fleet for screenshots; only shell tiles are live; nothing saved.
    Demo,
    /// The real thing: persisted state, control socket, reload on rebuild.
    Live,
    /// Read-only twin of a Live instance (same state file, same tmux server)
    /// used by `termdenctl screenshot` to render it headlessly.
    Mirror,
}

/// Local "HH:MM", `ago` seconds in the past.
pub fn clock(ago: u64) -> String {
    // SAFETY: localtime_r only writes into the zeroed tm we pass.
    unsafe {
        let t = libc::time(std::ptr::null_mut()) - ago as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    }
}

pub use termden_core::Activity;

pub struct TermdenApp {
    pub mode: Mode,
    pub config: Config,
    pub theme: Theme,
    pub workspaces: Vec<Workspace>,
    pub layouts: HashMap<WorkspaceId, LayoutNode>,
    pub active: WorkspaceId,
    pub focused: Option<HarnessId>,
    pub mail: Vec<termden_core::Mail>,
    /// Unread count each harness was last nudged about.
    pub mail_notified: HashMap<HarnessId, u16>,
    pub drag: Option<DragState>,
    pub drop_target: Option<(HarnessId, DropZone)>,
    /// Bumped on a timer; drives spinners and the cursor blink.
    pub tick: u64,
    /// Live terminals, keyed by harness. tmux owns the processes.
    pub sessions: HashMap<HarnessId, Session>,
    /// Keyboard focus for the whole window; keys go to the focused harness.
    pub focus: FocusHandle,
    pub activity: Vec<Activity>,
    pub activity_scroll: ScrollHandle,
    /// Set when a line is logged; the feed then scrolls to the newest on render.
    pub activity_stick: std::cell::Cell<bool>,
    /// Workspace whose "add harness" picker is open.
    pub adding: Option<WorkspaceId>,
    /// Workspace awaiting a second click to confirm removal.
    pub confirm_remove: Option<WorkspaceId>,
    /// Harness whose close was requested once; a second request within a few
    /// seconds kills it.
    pub confirm_close: Option<(HarnessId, Instant)>,
    /// Tile shown over the whole canvas (active workspace only).
    pub zoomed: Option<HarnessId>,
    /// Split divider being dragged.
    pub resizing: Option<canvas::Divider>,
    /// Crossing dividers dragged along when a junction was pressed.
    pub resizing_extra: Vec<canvas::Divider>,
    /// Dragging the edge of the left panel.
    pub panel_resizing: bool,
    pub settings_open: bool,
    pub popups: [crate::popup::PopupPos; 1],
    /// Questions agents asked the user, answered in the browser (`choices.rs`).
    pub choices: Vec<termden_core::Choice>,
    /// Absolute tile rects of the active workspace, from the last frame.
    pub tile_rects: Vec<(HarnessId, TRect)>,
    pub viewport: (f32, f32),
    /// Terminal cell size in px, measured from the mono font.
    pub cell: (f32, f32),
    /// Tile and button of an in-progress terminal mouse press.
    mouse_press: Option<(HarnessId, u8, (u16, u16))>,
    /// Link under the pointer while ctrl is held: (tile, row, start col, cells).
    hover_url: Option<(HarnessId, u16, u16, u16)>,
    /// Newer release tag offered in the footer (`update.rs`).
    pub update_available: Option<String>,
    pub updating: bool,
    scroll_lines: f32,
    /// Our own binary and its mtime at startup, for reload-on-rebuild.
    pub(crate) exe: Option<(PathBuf, SystemTime)>,
    pub started: Instant,
}

impl TermdenApp {
    pub fn new(config: Config, mode: Mode, cx: &mut Context<Self>) -> Self {
        let theme = Theme::for_id(config.theme, &config);

        let state = match mode {
            Mode::Demo => mock::build(),
            Mode::Live | Mode::Mirror => AppState::load().unwrap_or_else(actions::first_run),
        };
        let active = state
            .active
            .filter(|id| state.workspaces.iter().any(|w| w.id == *id))
            .or(state.workspaces.first().map(|w| w.id))
            .unwrap_or(WorkspaceId::new(0));

        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(120)).await;
                let updated = this.update(cx, |this, cx| hot!(Self::on_tick, this, cx));
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        if mode == Mode::Live {
            control::listen(cx);
            crate::choices::listen(cx);
            Self::watch_updates(cx);
        }
        // Warm the interactive-shell PATH probe before the first agent spawns.
        std::thread::spawn(|| pty::user_path());

        let exe = (mode == Mode::Live)
            .then(|| {
                let path = std::env::current_exe().ok()?;
                let mtime = std::fs::metadata(&path).ok()?.modified().ok()?;
                Some((path, mtime))
            })
            .flatten();

        let mut workspaces = state.workspaces;
        termden_core::assign_agent_colors(&mut workspaces, color_seed());
        let mut app = Self {
            mode,
            config,
            theme,
            workspaces,
            layouts: state.layouts,
            active,
            focused: state.focused,
            mail: state.mail,
            mail_notified: HashMap::new(),
            drag: None,
            drop_target: None,
            tick: 0,
            sessions: HashMap::new(),
            focus: cx.focus_handle(),
            activity: state.activity,
            activity_scroll: ScrollHandle::new(),
            activity_stick: std::cell::Cell::new(true),
            adding: None,
            confirm_remove: None,
            confirm_close: None,
            zoomed: None,
            resizing: None,
            resizing_extra: Vec::new(),
            panel_resizing: false,
            settings_open: false,
            popups: Default::default(),
            choices: state.choices,
            tile_rects: Vec::new(),
            viewport: (0.0, 0.0),
            cell: (7.2, 19.0),
            mouse_press: None,
            hover_url: None,
            update_available: None,
            updating: false,
            scroll_lines: 0.0,
            exe,
            started: Instant::now(),
        };
        if mode == Mode::Demo {
            mock::seed_activity(&mut app);
        } else {
            let verb = if std::env::var_os("TERMDEN_RELOADED").is_some() {
                "updated"
            } else {
                "started"
            };
            app.activity.retain(|e| e.name != "Termden");
            app.drop_placeholder_joins();
            app.log("Termden", verb.into(), 0);
        }
        app
    }

    /// Record window size and cell metrics; only a render has a `Window`.
    fn measure(&mut self, window: &mut Window) {
        let viewport = window.viewport_size();
        self.viewport = (f32::from(viewport.width), f32::from(viewport.height));
        let text = window.text_system();
        let font_id = text.resolve_font(&font(self.theme.mono_font.clone()));
        let cell_w = text
            .advance(font_id, px(self.theme.term_font), 'm')
            .map(|size| f32::from(size.width))
            .unwrap_or(7.2);
        self.cell = (cell_w, self.theme.term_line);
    }

    fn viewport_size(&self) -> gpui::Size<Pixels> {
        gpui::size(px(self.viewport.0), px(self.viewport.1))
    }

    /// Every tile of a workspace laid out on the canvas, ignoring zoom.
    pub fn layout_tiles(&self, ws: WorkspaceId) -> Vec<(HarnessId, TRect)> {
        let rect = canvas::canvas_rect(&self.config, self.viewport_size());
        self.layouts
            .get(&ws)
            .map(|layout| canvas::layout_rects(layout, rect, self.config.gap))
            .unwrap_or_default()
    }

    /// The tiles actually drawn: just the zoomed one, if any. Rendering,
    /// sessions and drop hit-testing all use this.
    pub fn visible_tiles(&self, ws: WorkspaceId) -> Vec<(HarnessId, TRect)> {
        let zoomed = self.zoomed.filter(|id| {
            ws == self.active && self.layouts.get(&ws).is_some_and(|l| l.contains(*id))
        });
        match zoomed {
            Some(id) => vec![(id, canvas::canvas_rect(&self.config, self.viewport_size()))],
            None => self.layout_tiles(ws),
        }
    }

    pub fn toggle_zoom(&mut self) {
        self.zoomed = match (self.zoomed, self.focused) {
            (Some(_), _) => None,
            (None, focused) => focused,
        };
    }

    /// Close on the second request (button or shortcut) within 3 seconds.
    pub fn request_close(&mut self, id: HarnessId) {
        match self.confirm_close {
            Some((armed, at)) if armed == id && at.elapsed() < Duration::from_secs(3) => {
                self.confirm_close = None;
                self.close_harness(id);
            }
            _ => self.confirm_close = Some((id, Instant::now())),
        }
    }

    pub fn close_armed(&self, id: HarnessId) -> bool {
        self.confirm_close
            .is_some_and(|(armed, at)| armed == id && at.elapsed() < Duration::from_secs(3))
    }

    /// Focus the nearest tile in a direction (by tile centres).
    fn focus_direction(&mut self, dx: f32, dy: f32) {
        let tiles = self.layout_tiles(self.active);
        let Some(from) = self
            .focused
            .and_then(|f| tiles.iter().find(|(id, _)| *id == f))
            .map(|(_, r)| *r)
        else {
            if let Some((id, _)) = tiles.first() {
                self.focus_harness(*id);
            }
            return;
        };
        let centre = |r: &TRect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let (fx, fy) = centre(&from);
        let best = tiles
            .iter()
            .filter_map(|(id, rect)| {
                let (x, y) = centre(rect);
                let (vx, vy) = (x - fx, y - fy);
                let along = vx * dx + vy * dy;
                // Ahead of us, and more ahead than sideways.
                (along > 1.0 && along >= (vx * dy - vy * dx).abs() * 0.5)
                    .then(|| (*id, along + (vx * dy - vy * dx).abs() * 2.0))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((id, _)) = best {
            self.focus_harness(id);
        }
    }

    fn cycle_workspace(&mut self, step: isize) {
        let len = self.workspaces.len() as isize;
        if len == 0 {
            return;
        }
        let index = self
            .workspaces
            .iter()
            .position(|w| w.id == self.active)
            .unwrap_or(0) as isize;
        let next = self.workspaces[(index + step).rem_euclid(len) as usize].id;
        self.activate_workspace(next);
    }

    /// Spawn sessions for visible tiles and keep every visible grid sized to
    /// its tile. Runs every frame and every tick (a window hidden on another
    /// workspace gets no frames, but harnesses added over the control socket
    /// must still start). Resizes are no-ops when unchanged.
    pub fn sync_sessions(&mut self, cx: &mut Context<Self>) {
        if self.viewport.0 <= 0.0 {
            return;
        }
        let cell = self.cell;
        self.tile_rects = self.visible_tiles(self.active);

        // Hidden tiles (other workspaces, behind a zoom) stay attached and
        // sized to their own layout so `screen`/`send` work on them too. The
        // demo keeps to what is on screen.
        let mut tiles = self.tile_rects.clone();
        if self.mode != Mode::Demo {
            for ws in self.workspaces.iter().map(|w| w.id).collect::<Vec<_>>() {
                for (id, rect) in self.layout_tiles(ws) {
                    if !tiles.iter().any(|(t, _)| *t == id) {
                        tiles.push((id, rect));
                    }
                }
            }
        }

        for (id, rect) in tiles {
            let (cols, rows) = tile::grid_size(rect, cell);
            if let Some(session) = self.sessions.get_mut(&id) {
                session.resize(cols, rows, cell);
                continue;
            }
            let Some((workspace, harness)) = self.harness(id) else {
                continue;
            };
            let live = self.mode != Mode::Demo || harness.kind == HarnessKind::Shell;
            if !live || harness.status == HarnessStatus::Error {
                continue;
            }

            let cwd = resolve_cwd(&workspace.path, &harness.cwd);
            let command = match harness.kind {
                HarnessKind::Shell => None,
                kind if self.mode == Mode::Mirror => Some(kind.default_command().to_string()),
                kind => Some(launch::command(id, &harness.name, kind)),
            };
            match Session::spawn(
                id,
                cwd,
                command.as_deref(),
                self.mode == Mode::Mirror,
                cols,
                rows,
                cell,
            ) {
                Ok((session, events)) => {
                    self.sessions.insert(id, session);
                    cx.spawn(async move |this, cx| {
                        while let Ok(event) = events.recv().await {
                            let alive = this.update(cx, |this, cx| {
                                this.on_term_event(id, event, cx);
                                cx.notify();
                            });
                            if alive.is_err() {
                                break;
                            }
                        }
                    })
                    .detach();
                }
                Err(error) => {
                    eprintln!("termden: could not start harness {}: {error}", id.raw());
                    self.set_status(id, HarnessStatus::Error);
                    self.log_harness(id, "could not start".into());
                }
            }
        }
    }

    fn on_term_event(&mut self, id: HarnessId, event: TermEvent, cx: &mut Context<Self>) {
        let focused = self.focused == Some(id);
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        match event {
            TermEvent::Wakeup => session.last_output = Instant::now(),
            TermEvent::PtyWrite(text) => session.write(text.into_bytes()),
            TermEvent::Title(title) => session.title = title,
            TermEvent::ResetTitle => session.title.clear(),
            TermEvent::ClipboardStore(_, text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            TermEvent::Bell if !focused && !session.attention => {
                session.attention = true;
                self.log_harness(id, "needs your attention".into());
            }
            TermEvent::ChildExit(status) => {
                session.exited = true;
                let failed = !status.success();
                self.set_status(
                    id,
                    if failed {
                        HarnessStatus::Error
                    } else {
                        HarnessStatus::Idle
                    },
                );
                let message = if failed {
                    "stopped unexpectedly"
                } else {
                    "finished"
                };
                self.log_harness(id, message.into());
            }
            _ => {}
        }
    }

    /// Derive harness status from session activity until adapters report it.
    fn refresh_statuses(&mut self) {
        let mut updates = Vec::new();
        for (id, session) in &self.sessions {
            if session.exited {
                continue;
            }
            let quiet_typing = session
                .last_output
                .saturating_duration_since(session.last_input)
                > Duration::from_millis(300);
            let status = if session.attention {
                HarnessStatus::NeedsInput
            } else if session.last_output.elapsed() < Duration::from_millis(1500) && quiet_typing {
                HarnessStatus::Working
            } else {
                HarnessStatus::Idle
            };
            updates.push((*id, status));
        }
        for (id, status) in updates {
            self.set_status(id, status);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        if self.settings_open {
            if keystroke.key == "escape" {
                self.settings_open = false;
            }
            return true;
        }
        if m.control && m.shift {
            match keystroke.key.as_str() {
                "v" => {
                    let text = cx.read_from_clipboard().and_then(|item| item.text());
                    if let (Some(text), Some(session)) = (text, self.focused_session()) {
                        session.paste(&text);
                    }
                    return true;
                }
                "," => {
                    self.settings_open = true;
                    return true;
                }
                "t" => {
                    self.add_harness(self.active, HarnessKind::Shell);
                    return true;
                }
                "w" => {
                    if let Some(id) = self.focused {
                        self.request_close(id);
                    }
                    return true;
                }
                "z" => {
                    self.toggle_zoom();
                    return true;
                }
                "=" | "+" => {
                    self.set_font_size(self.config.font_size + 1.0);
                    return true;
                }
                "-" | "_" => {
                    self.set_font_size(self.config.font_size - 1.0);
                    return true;
                }
                "0" | ")" => {
                    self.set_font_size(Config::default().font_size);
                    return true;
                }
                "left" | "h" => {
                    self.focus_direction(-1.0, 0.0);
                    return true;
                }
                "right" | "l" => {
                    self.focus_direction(1.0, 0.0);
                    return true;
                }
                "up" | "k" => {
                    self.focus_direction(0.0, -1.0);
                    return true;
                }
                "down" | "j" => {
                    self.focus_direction(0.0, 1.0);
                    return true;
                }
                "pageup" => {
                    self.cycle_workspace(-1);
                    return true;
                }
                "pagedown" => {
                    self.cycle_workspace(1);
                    return true;
                }
                "r" if self.mode == Mode::Live => {
                    self.reload();
                    return true;
                }
                // Copy happens on mouse release (tmux selection -> OSC 52);
                // never let the shortcut leak through as ^C.
                "c" => return true,
                _ => {}
            }
        }

        let Some(session) = self.focused_session() else {
            return false;
        };
        if session.exited {
            // Enter on an exited tile starts it again.
            if keystroke.key == "enter" && !m.modified() {
                if let Some(id) = self.focused {
                    self.restart_harness(id);
                }
                return true;
            }
            return false;
        }
        let app_cursor = session.term.lock().mode().contains(TermMode::APP_CURSOR);
        let Some(bytes) = pty::key_bytes(keystroke, app_cursor) else {
            return false;
        };
        let clearing = keystroke.key == "enter" && !m.modified() && session.cursor_line_is_clear();
        session.input(bytes);
        if clearing && let Some(id) = self.focused {
            self.reset_identity(id);
        }
        true
    }

    fn focused_session(&mut self) -> Option<&mut Session> {
        self.sessions.get_mut(&self.focused?)
    }

    /// Grid cell under a window position, clamped to the tile.
    fn cell_at(&self, id: HarnessId, position: Point<Pixels>) -> Option<(u16, u16)> {
        let (_, rect) = self.tile_rects.iter().find(|(tile, _)| *tile == id)?;
        let session = self.sessions.get(&id)?;
        let (x, y) = tile::content_origin(*rect, session.rows, self.cell.1);
        let col = ((f32::from(position.x) - x) / self.cell.0).max(0.0) as u16;
        let row = ((f32::from(position.y) - y) / self.cell.1).max(0.0) as u16;
        Some((
            col.min(session.cols.saturating_sub(1)),
            row.min(session.rows.saturating_sub(1)),
        ))
    }

    /// Track the link a ctrl+click would open, for the hover underline.
    fn update_hover_url(&mut self, ctrl: bool, position: Point<Pixels>) {
        let (x, y) = (f32::from(position.x), f32::from(position.y));
        self.hover_url = ctrl
            .then(|| self.tile_rects.iter().find(|(_, r)| r.contains(x, y)))
            .flatten()
            .and_then(|&(id, _)| {
                let (col, row) = self.cell_at(id, position)?;
                let (start, len, _) = self.sessions.get(&id)?.url_at(col, row)?;
                Some((id, row, start, len))
            });
    }

    /// Underline (and hand cursor) over the hovered link.
    fn hover_url_element(&self) -> Option<AnyElement> {
        let (id, row, start, len) = self.hover_url?;
        let (_, rect) = self.tile_rects.iter().find(|(tile, _)| *tile == id)?;
        let session = self.sessions.get(&id)?;
        let (x, y) = tile::content_origin(*rect, session.rows, self.cell.1);
        Some(
            div()
                .id("hover-url")
                .absolute()
                .left(px(x + f32::from(start) * self.cell.0))
                .top(px(y + f32::from(row) * self.cell.1))
                .w(px(f32::from(len) * self.cell.0))
                .h(px(self.cell.1))
                .border_b_1()
                .border_color(self.theme.blue)
                .cursor_pointer()
                .into_any_element(),
        )
    }

    /// Ctrl+click: open the URL under the pointer instead of clicking into tmux.
    pub fn open_url_at(&self, id: HarnessId, position: Point<Pixels>) -> bool {
        let Some(url) = self
            .cell_at(id, position)
            .and_then(|(col, row)| self.sessions.get(&id)?.url_at(col, row))
            .map(|(_, _, url)| url)
        else {
            return false;
        };
        if let Ok(mut child) = std::process::Command::new("xdg-open").arg(url).spawn() {
            std::thread::spawn(move || child.wait());
        }
        true
    }

    pub fn terminal_mouse_down(
        &mut self,
        id: HarnessId,
        button: MouseButton,
        position: Point<Pixels>,
    ) {
        let code = match button {
            MouseButton::Left => pty::MOUSE_LEFT,
            MouseButton::Middle => pty::MOUSE_MIDDLE,
            MouseButton::Right => pty::MOUSE_RIGHT,
            _ => return,
        };
        let Some(cell) = self.cell_at(id, position) else {
            return;
        };
        if let Some(session) = self.sessions.get_mut(&id) {
            session.mouse(code, cell.0, cell.1, true);
            self.mouse_press = Some((id, code, cell));
        }
    }

    pub fn terminal_mouse_drag(&mut self, position: Point<Pixels>) {
        let Some((id, code, last)) = self.mouse_press else {
            return;
        };
        let Some(cell) = self.cell_at(id, position) else {
            return;
        };
        if cell == last {
            return;
        }
        if let Some(session) = self.sessions.get_mut(&id) {
            if session.wants_drag() {
                session.mouse(code | pty::MOUSE_DRAG, cell.0, cell.1, true);
            }
        }
        self.mouse_press = Some((id, code, cell));
    }

    pub fn terminal_mouse_up(&mut self, position: Point<Pixels>) {
        let Some((id, code, last)) = self.mouse_press.take() else {
            return;
        };
        let cell = self.cell_at(id, position).unwrap_or(last);
        if let Some(session) = self.sessions.get_mut(&id) {
            session.mouse(code, cell.0, cell.1, false);
        }
    }

    pub fn terminal_scroll(&mut self, id: HarnessId, event: &ScrollWheelEvent) {
        let Some(cell) = self.cell_at(id, event.position) else {
            return;
        };
        let line = px(self.cell.1);
        self.scroll_lines += f32::from(event.delta.pixel_delta(line).y) / self.cell.1;
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        // tmux scrolls several lines per wheel report; one report per line
        // of accumulated delta keeps trackpads and wheels both usable.
        while self.scroll_lines.abs() >= 1.0 {
            let up = self.scroll_lines > 0.0;
            session.mouse(
                if up { pty::WHEEL_UP } else { pty::WHEEL_DOWN },
                cell.0,
                cell.1,
                true,
            );
            self.scroll_lines -= self.scroll_lines.signum();
        }
    }

    pub fn set_status(&mut self, id: HarnessId, status: HarnessStatus) {
        let mut first_work = false;
        if let Some(harness) = self
            .workspaces
            .iter_mut()
            .find_map(|workspace| workspace.harness_mut(id))
        {
            harness.status = status;
            if status == HarnessStatus::Working
                && harness.kind != HarnessKind::Shell
                // Startup output is not work: wait until the agent has said
                // who it is (or was named) before reporting its first turn.
                && harness.announced
                && !harness.started
            {
                harness.started = true;
                first_work = true;
            }
        }
        if first_work {
            self.log_harness(id, "started working".into());
        }
    }

    pub fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    pub fn workspace_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces
            .iter_mut()
            .find(|workspace| workspace.id == id)
    }

    pub fn harness(&self, id: HarnessId) -> Option<(&Workspace, &Harness)> {
        self.workspaces
            .iter()
            .find_map(|workspace| workspace.harness(id).map(|harness| (workspace, harness)))
    }

    pub fn activate_workspace(&mut self, id: WorkspaceId) {
        self.active = id;
        if let Some(workspace) = self.workspace_mut(id) {
            workspace.expanded = true;
        }

        let focused_in_workspace = self
            .focused
            .and_then(|focused| self.workspace(id).and_then(|ws| ws.harness(focused)))
            .is_some();
        if !focused_in_workspace {
            self.focused = self.layouts.get(&id).and_then(|layout| layout.first_leaf());
        }
        self.clear_attention();
        self.save();
    }

    pub fn toggle_workspace(&mut self, id: WorkspaceId) {
        if let Some(workspace) = self.workspace_mut(id) {
            workspace.expanded = !workspace.expanded;
        }
        self.save();
    }

    pub fn focus_harness(&mut self, id: HarnessId) {
        let workspace_id = self.harness(id).map(|(workspace, _)| workspace.id);
        if let Some(workspace_id) = workspace_id {
            self.active = workspace_id;
            if let Some(workspace) = self.workspace_mut(workspace_id) {
                workspace.expanded = true;
            }
            self.focused = Some(id);
            if self.zoomed.is_some() {
                self.zoomed = Some(id);
            }
            self.clear_attention();
            self.save();
        }
    }

    fn clear_attention(&mut self) {
        if let Some(session) = self.focused.and_then(|id| self.sessions.get_mut(&id)) {
            session.attention = false;
        }
    }

    pub fn set_font_size(&mut self, size: f32) {
        self.config.font_size = size.clamp(8.0, 24.0);
        self.theme = Theme::for_id(self.config.theme, &self.config);
        if let Err(error) = self.config.save() {
            eprintln!("termden: could not save config: {error}");
        }
    }

    pub fn cycle_theme(&mut self) {
        self.set_theme(self.config.theme.next());
    }

    pub fn set_theme(&mut self, id: termden_core::ThemeId) {
        self.config.theme = id;
        self.theme = Theme::for_id(id, &self.config);
        if let Err(error) = self.config.save() {
            eprintln!("termden: could not save config: {error}");
        }
    }

    pub fn begin_drag(&mut self, harness: HarnessId, position: Point<Pixels>) {
        self.focused = Some(harness);
        self.drag = Some(DragState {
            harness,
            start: position,
            pos: position,
            active: false,
        });
        self.drop_target = None;
    }

    pub fn on_mouse_move(&mut self, position: Point<Pixels>, window: &Window) {
        let mouse = (f32::from(position.x), f32::from(position.y));
        if let Some(p) = self.popups.iter_mut().find(|p| p.dragging()) {
            p.drag_to(mouse, self.viewport);
            return;
        }
        if self.panel_resizing {
            let gap = self.config.gap;
            let max = (f32::from(self.viewport.0) * 0.5).max(220.0);
            self.config.panel_width = (f32::from(position.x) - gap * 1.5).clamp(200.0, max);
            return;
        }
        if let Some(divider) = &self.resizing {
            let pos = match divider.axis {
                termden_core::Axis::Horizontal => f32::from(position.x),
                termden_core::Axis::Vertical => f32::from(position.y),
            };
            let ratio = canvas::ratio_at(divider, self.config.gap, pos);
            let mut updates = vec![(divider.path.clone(), ratio)];
            for extra in &self.resizing_extra {
                let pos = match extra.axis {
                    termden_core::Axis::Horizontal => f32::from(position.x),
                    termden_core::Axis::Vertical => f32::from(position.y),
                };
                updates.push((
                    extra.path.clone(),
                    canvas::ratio_at(extra, self.config.gap, pos),
                ));
            }
            if let Some(layout) = self.layouts.get_mut(&self.active) {
                for (path, ratio) in updates {
                    layout.set_ratio(&path, ratio);
                }
            }
            return;
        }
        let Some(drag) = self.drag.as_mut() else {
            return;
        };

        drag.pos = position;
        if !drag.active {
            let dx = f32::from(position.x) - f32::from(drag.start.x);
            let dy = f32::from(position.y) - f32::from(drag.start.y);
            if dx * dx + dy * dy > 64.0 {
                drag.active = true;
            }
        }

        let active = drag.active;
        let dragged = drag.harness;
        if active {
            self.drop_target = self.hit_test(dragged, position, window);
        }
    }

    pub fn on_mouse_up(&mut self) {
        self.popups.iter_mut().for_each(|p| p.end_drag());
        if std::mem::take(&mut self.panel_resizing) {
            if let Err(error) = self.config.save() {
                eprintln!("termden: could not save config: {error}");
            }
            return;
        }
        if self.resizing.take().is_some() {
            self.resizing_extra.clear();
            self.save();
            return;
        }
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some((target, zone)) = self.drop_target.take() else {
            return;
        };
        if !drag.active {
            return;
        }

        let active = self.active;
        if let Some(layout) = self.layouts.get_mut(&active) {
            if layout.move_leaf(drag.harness, target, zone) {
                self.focused = Some(drag.harness);
            }
        }
        self.save();
    }

    fn hit_test(
        &self,
        dragged: HarnessId,
        position: Point<Pixels>,
        window: &Window,
    ) -> Option<(HarnessId, DropZone)> {
        let _ = window;
        let x = f32::from(position.x);
        let y = f32::from(position.y);

        for (id, tile) in self.visible_tiles(self.active) {
            if id == dragged || !tile.contains(x, y) {
                continue;
            }

            let rx = (x - tile.x) / tile.w.max(1.0);
            let ry = (y - tile.y) / tile.h.max(1.0);
            let zone = if rx < 0.25 {
                DropZone::Left
            } else if rx > 0.75 {
                DropZone::Right
            } else if ry < 0.25 {
                DropZone::Top
            } else if ry > 0.75 {
                DropZone::Bottom
            } else {
                DropZone::Center
            };
            return Some((id, zone));
        }

        None
    }

    fn render_ghost(&self) -> Option<AnyElement> {
        let drag = self.drag.as_ref()?;
        if !drag.active {
            return None;
        }
        let (workspace, harness) = self.harness(drag.harness)?;
        let theme = &self.theme;
        let accent = theme.accent(workspace.accent);
        let left = f32::from(drag.pos.x) - 100.0;
        let top = f32::from(drag.pos.y) - 26.0;

        Some(
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(200.))
                .h(px(56.))
                .rounded(px(10.))
                .bg(theme.base)
                .border_2()
                .border_color(accent)
                .shadow_lg()
                .opacity(0.94)
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .h(px(32.))
                        .px(px(9.))
                        .bg(theme.surface0.opacity(0.4))
                        .child(dot(theme.status(harness.status), 6.))
                        .child(
                            div()
                                .text_size(theme.ui(11.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(harness.name.clone()),
                        )
                        .child(kind_badge(harness.kind, theme))
                        .child(hspacer())
                        .child(
                            div()
                                .text_size(theme.ui(9.))
                                .text_color(accent)
                                .child(workspace.name.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .px(px(10.))
                        .h(px(24.))
                        .text_size(theme.ui(9.))
                        .text_color(theme.overlay)
                        .child("drop on an edge to split · center to swap"),
                ),
        )
        .map(|element| element.into_any_element())
    }
}

impl Render for TermdenApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        hot!(Self::render_body, self, window, cx)
    }
}

impl TermdenApp {
    /// The 120ms frame tick: spinners, statuses, mail, rebuild check.
    fn on_tick(&mut self, cx: &mut Context<Self>) {
        self.tick = self.tick.wrapping_add(1);
        self.sync_sessions(cx);
        self.refresh_statuses();
        self.notify_mail(cx);
        if self.tick % 8 == 0 {
            self.check_rebuilt();
        }
        cx.notify();
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.measure(window);
        self.sync_sessions(cx);
        let gap = self.config.gap;
        self.popups[crate::popup::SETTINGS].update(self.settings_open);
        let panel = panel::render(self, cx);
        let canvas = canvas::render(self, window, cx);
        let ghost = self.render_ghost();

        div()
            .id("root")
            .relative()
            .size_full()
            .bg(self.theme.crust)
            .font_family(self.theme.ui_font.clone())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.on_key_down(event, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if event.pressed_button.is_some() {
                    this.terminal_mouse_drag(event.position);
                }
                this.on_mouse_move(event.position, window);
                this.update_hover_url(event.modifiers.control, event.position);
                cx.notify();
            }))
            .on_modifiers_changed(cx.listener(
                |this, event: &gpui::ModifiersChangedEvent, window, cx| {
                    this.update_hover_url(event.modifiers.control, window.mouse_position());
                    cx.notify();
                },
            ))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.terminal_mouse_up(event.position);
                    this.on_mouse_up();
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.terminal_mouse_up(event.position);
                    this.on_mouse_up();
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, event: &MouseUpEvent, _, _| {
                    this.terminal_mouse_up(event.position)
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseUpEvent, _, _| {
                    this.terminal_mouse_up(event.position)
                }),
            )
            .child(
                div()
                    .size_full()
                    .p(px(gap))
                    .flex()
                    .flex_row()
                    .gap(px(gap))
                    .child(panel)
                    .child(canvas),
            )
            .child(
                div()
                    .id("panel-resize")
                    .absolute()
                    .left(px(gap + self.config.panel_width))
                    .top(px(gap))
                    .bottom(px(gap))
                    .w(px(gap))
                    .cursor_col_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.panel_resizing = true;
                            cx.stop_propagation();
                        }),
                    ),
            )
            .children(self.hover_url_element())
            .children(crate::settings::render(self, cx))
            .children(ghost)
            .into_any_element()
    }
}

/// Workspace path joined with the harness cwd, `~` expanded. Falls back to
/// `$HOME` when the folder does not exist (the mock fleet points at some).
pub fn resolve_cwd(root: &str, cwd: &str) -> PathBuf {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()));
    let root = match root.strip_prefix("~") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(root),
    };
    let path = root.join(cwd);
    if path.is_dir() { path } else { home }
}

pub fn color_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}
