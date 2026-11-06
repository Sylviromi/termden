use gpui::{AnyElement, Context, Pixels, Size, div, prelude::*, px};
use termden_core::{Axis, Config, HarnessId, LayoutNode};

use crate::state::TermdenApp;
use crate::tile;

/// A simple rectangle in window coordinates. Kept separate from GPUI's
/// `Bounds` so layout math stays plain and testable.
#[derive(Clone, Copy, Debug)]
pub struct TRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl TRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }

    pub fn relative_to(&self, parent: TRect) -> TRect {
        TRect {
            x: self.x - parent.x,
            y: self.y - parent.y,
            w: self.w,
            h: self.h,
        }
    }
}

/// Where the tiling canvas sits inside the window.
pub fn canvas_rect(config: &Config, viewport: Size<Pixels>) -> TRect {
    let gap = config.gap;
    let x = gap + config.panel_width + gap;
    let width = f32::from(viewport.width) - x - gap;
    let height = f32::from(viewport.height) - gap * 2.0;

    TRect {
        x,
        y: gap,
        w: width.max(1.0),
        h: height.max(1.0),
    }
}

pub fn layout_rects(node: &LayoutNode, rect: TRect, gap: f32) -> Vec<(HarnessId, TRect)> {
    let mut out = Vec::new();
    walk(node, rect, gap, &mut Vec::new(), &mut out, &mut Vec::new());
    out
}

/// The gap between the two halves of a split; dragging it sets the ratio.
#[derive(Clone, Debug)]
pub struct Divider {
    /// Route from the root: `false` = first child, `true` = second.
    pub path: Vec<bool>,
    pub axis: Axis,
    /// The whole split, to turn a pointer position into a ratio.
    pub split: TRect,
    /// The gap itself.
    pub rect: TRect,
}

pub fn dividers(node: &LayoutNode, rect: TRect, gap: f32) -> Vec<Divider> {
    let mut out = Vec::new();
    walk(node, rect, gap, &mut Vec::new(), &mut Vec::new(), &mut out);
    out
}

/// Ratio that puts a split's divider under `pos` (along its axis).
pub fn ratio_at(divider: &Divider, gap: f32, pos: f32) -> f32 {
    let (start, extent) = match divider.axis {
        Axis::Horizontal => (divider.split.x, divider.split.w),
        Axis::Vertical => (divider.split.y, divider.split.h),
    };
    ((pos - start - gap / 2.0) / (extent - gap).max(1.0)).clamp(0.1, 0.9)
}

fn walk(
    node: &LayoutNode,
    rect: TRect,
    gap: f32,
    path: &mut Vec<bool>,
    tiles: &mut Vec<(HarnessId, TRect)>,
    dividers: &mut Vec<Divider>,
) {
    let LayoutNode::Split {
        axis,
        ratio,
        first,
        second,
    } = node
    else {
        if let LayoutNode::Leaf(id) = node {
            tiles.push((*id, rect));
        }
        return;
    };
    let horizontal = *axis == Axis::Horizontal;
    let available = if horizontal { rect.w } else { rect.h } - gap;
    let available = available.max(1.0);
    let min = 90.0_f32.min(available / 2.0);
    let first_size = (available * ratio).clamp(min, available - min);
    let second_size = available - first_size;

    let (a, gap_rect, b) = if horizontal {
        (
            TRect {
                w: first_size,
                ..rect
            },
            TRect {
                x: rect.x + first_size,
                w: gap,
                ..rect
            },
            TRect {
                x: rect.x + first_size + gap,
                w: second_size,
                ..rect
            },
        )
    } else {
        (
            TRect {
                h: first_size,
                ..rect
            },
            TRect {
                y: rect.y + first_size,
                h: gap,
                ..rect
            },
            TRect {
                y: rect.y + first_size + gap,
                h: second_size,
                ..rect
            },
        )
    };
    dividers.push(Divider {
        path: path.clone(),
        axis: *axis,
        split: rect,
        rect: gap_rect,
    });
    for (child, child_rect, side) in [(first, a, false), (second, b, true)] {
        path.push(side);
        walk(child, child_rect, gap, path, tiles, dividers);
        path.pop();
    }
}

pub fn render(app: &TermdenApp, window: &gpui::Window, cx: &mut Context<TermdenApp>) -> AnyElement {
    let theme = &app.theme;
    let rect = canvas_rect(&app.config, window.viewport_size());
    let tiles = app.visible_tiles(app.active);
    let empty = tiles.is_empty();

    div()
        .id("canvas")
        .flex_1()
        .relative()
        .overflow_hidden()
        .rounded(px(app.config.corner_radius))
        .bg(theme.crust)
        .children(
            tiles
                .into_iter()
                .map(|(id, tile_rect)| tile::render(app, id, tile_rect.relative_to(rect), cx)),
        )
        .children(divider_handles(app, rect, cx))
        .children(empty.then(|| empty_state(app)))
        .into_any_element()
}

fn empty_state(app: &TermdenApp) -> AnyElement {
    let theme = &app.theme;

    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .child(
            div()
                .w(px(38.))
                .h(px(38.))
                .rounded(px(11.))
                .border_1()
                .border_color(theme.text.opacity(0.08))
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.ui(16.))
                .text_color(theme.overlay)
                .font_family(crate::widgets::ICON_FONT)
                .child(crate::widgets::icons::TERMINAL),
        )
        .child(
            div()
                .text_size(theme.ui(12.))
                .text_color(theme.subtext)
                .child("No harnesses in this workspace"),
        )
        .child(
            div()
                .text_size(theme.ui(10.))
                .text_color(theme.overlay)
                .child("start one from the panel or ask an agent"),
        )
        .into_any_element()
}

/// Invisible handles over the gaps between tiles; they light up on hover and
/// while dragging.
fn divider_handles(
    app: &TermdenApp,
    canvas: TRect,
    cx: &mut Context<TermdenApp>,
) -> Vec<AnyElement> {
    if app.zoomed.is_some() || app.drag.is_some_and(|d| d.active) {
        return Vec::new();
    }
    let Some(layout) = app.layouts.get(&app.active) else {
        return Vec::new();
    };
    let theme = &app.theme;
    let accent = app
        .workspace(app.active)
        .map_or(theme.accent(0), |w| theme.accent(w.accent));
    let active_path = app.resizing.as_ref().map(|d| d.path.clone());
    let all = dividers(layout, canvas, app.config.gap);
    let gap = app.config.gap;
    all.iter()
        .cloned()
        .enumerate()
        .map(|(index, divider)| {
            let all = all.clone();
            let rect = divider.rect.relative_to(canvas);
            let horizontal = divider.axis == Axis::Horizontal;
            let active = active_path.as_ref() == Some(&divider.path);
            let bar = div()
                .rounded(px(2.))
                .bg(accent.opacity(if active { 0.9 } else { 0.0 }));
            let bar = if horizontal {
                bar.w(px(3.)).h(px((rect.h * 0.3).min(80.)))
            } else {
                bar.h(px(3.)).w(px((rect.w * 0.3).min(80.)))
            };
            let handle = div()
                .id(("divider", index))
                .absolute()
                .left(px(rect.x))
                .top(px(rect.y))
                .w(px(rect.w))
                .h(px(rect.h))
                .flex()
                .items_center()
                .justify_center()
                .group("divider")
                .child(bar.group_hover("divider", move |s| s.bg(accent.opacity(0.6))))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                        // Pressing a junction drags the crossing dividers too (both axes).
                        this.resizing_extra = all
                            .iter()
                            .filter(|o| o.axis != divider.axis)
                            .filter(|o| {
                                TRect {
                                    x: o.rect.x - gap,
                                    y: o.rect.y - gap,
                                    w: o.rect.w + gap * 2.0,
                                    h: o.rect.h + gap * 2.0,
                                }
                                .contains(x, y)
                            })
                            .cloned()
                            .collect();
                        this.resizing = Some(divider.clone());
                        cx.stop_propagation();
                    }),
                );
            if horizontal {
                handle.cursor_col_resize().into_any_element()
            } else {
                handle.cursor_row_resize().into_any_element()
            }
        })
        .collect()
}
