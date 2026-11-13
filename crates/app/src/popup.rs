//! Shared popup frame: draggable header, centered-by-default position, open animation.

use std::time::{Duration, Instant};

use gpui::{
    Animation, AnimationExt, AnyElement, Context, MouseButton, MouseDownEvent, Stateful, div,
    ease_out_quint, prelude::*, px,
};

use crate::state::TermdenApp;

pub const SETTINGS: usize = 0;
pub const UPDATE: usize = 1;

const ANIM: Duration = Duration::from_millis(180);
const MARGIN: f32 = 60.0;
const MIN_HEADER: f32 = 54.0;

#[derive(Default)]
pub struct PopupPos {
    /// Offset from the centered position.
    pub offset: (f32, f32),
    /// (mouse position, offset) at drag start, plus card size for clamping.
    grab: Option<((f32, f32), (f32, f32), (f32, f32))>,
    was_open: bool,
    closed_at: Option<Instant>,
}

impl PopupPos {
    /// Call once per render with the popup's open flag: starts the close
    /// animation on open -> closed and resets the position once it finished.
    pub fn update(&mut self, open: bool) {
        if open {
            self.closed_at = None;
        } else if self.was_open {
            self.closed_at = Some(Instant::now());
        } else if self.closed_at.is_some_and(|t| t.elapsed() >= ANIM) {
            *self = Self::default();
        }
        self.was_open = open;
    }

    pub fn closing(&self) -> bool {
        self.closed_at.is_some_and(|t| t.elapsed() < ANIM)
    }

    pub fn dragging(&self) -> bool {
        self.grab.is_some()
    }

    pub fn end_drag(&mut self) {
        self.grab = None;
    }

    pub fn drag_to(&mut self, mouse: (f32, f32), viewport: (f32, f32)) {
        if let Some((from, start, size)) = self.grab {
            self.offset = clamp_offset(
                (start.0 + mouse.0 - from.0, start.1 + mouse.1 - from.1),
                viewport,
                size,
            );
        }
    }
}

/// Keeps the header on screen: at least `MARGIN` px of the card horizontally and
/// the header strip vertically. `size.1` is the card's max height.
pub fn clamp_offset(offset: (f32, f32), viewport: (f32, f32), size: (f32, f32)) -> (f32, f32) {
    let (vw, vh) = viewport;
    let (w, h) = (size.0, size.1.min(vh));
    let x = offset
        .0
        .clamp(MARGIN - vw / 2.0 - w / 2.0, vw / 2.0 + w / 2.0 - MARGIN);
    let y = offset
        .1
        .clamp(h / 2.0 - vh / 2.0, vh / 2.0 + MIN_HEADER / 2.0 - 40.0);
    (x, y)
}

/// Makes `header` the drag handle of popup `which`.
pub fn drag_handle(
    header: gpui::Div,
    which: usize,
    size: (f32, f32),
    cx: &mut Context<TermdenApp>,
) -> Stateful<gpui::Div> {
    header
        .id(("popup-header", which))
        .cursor_move()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                let pos = &mut this.popups[which];
                pos.grab = Some((
                    (f32::from(event.position.x), f32::from(event.position.y)),
                    pos.offset,
                    size,
                ));
                cx.stop_propagation();
            }),
        )
}

/// Backdrop + positioned, animated card. Clicking the backdrop runs `close`.
pub fn frame(
    id: &'static str,
    which: usize,
    app: &TermdenApp,
    card: Stateful<gpui::Div>,
    close: impl Fn(&mut TermdenApp) + 'static,
    cx: &mut Context<TermdenApp>,
) -> AnyElement {
    let (dx, dy) = app.popups[which].offset;
    let closing = app.popups[which].closing();
    let anim = || Animation::new(ANIM).with_easing(ease_out_quint());
    let card = card
        .relative()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .with_animation((id, 1 + 2 * usize::from(closing)), anim(), move |el, t| {
            let t = if closing { 1.0 - t } else { t };
            el.opacity(t).left(px(dx)).top(px(dy + (1.0 - t) * 12.0))
        });
    let crust = app.theme.crust;
    div()
        .id(id)
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                close(this);
                cx.notify();
            }),
        )
        .on_mouse_move(
            cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                this.on_mouse_move(event.position, window);
                cx.notify();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.popups.iter_mut().for_each(|p| p.end_drag());
                cx.notify();
            }),
        )
        .child(card)
        .with_animation((id, 2 * usize::from(closing)), anim(), move |el, t| {
            let t = if closing { 1.0 - t } else { t };
            el.bg(crust.opacity(0.62 * t))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_header_visible() {
        let vp = (1000.0, 800.0);
        let size = (500.0, 600.0);
        assert_eq!(clamp_offset((0.0, 0.0), vp, size), (0.0, 0.0));
        let (x, y) = clamp_offset((9999.0, 9999.0), vp, size);
        assert!(500.0 + x - 250.0 <= 1000.0 - MARGIN + 0.1);
        assert!(400.0 + y - 27.0 <= 800.0 - 40.0 + 0.1);
        let (x, y) = clamp_offset((-9999.0, -9999.0), vp, size);
        assert!(500.0 + x + 250.0 >= MARGIN - 0.1);
        assert!(400.0 + y - 300.0 >= -0.1);
    }
}
