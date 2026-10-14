//! Pointers drawn by the app. egui can only ask the OS for a stock cursor, so the panels ask for one
//! of ours with [`request`] while they work out what is under the pointer, and [`finish`], at the end
//! of the frame, hides the OS cursor and paints the request at the pointer ([`shown`] says what it
//! painted). A frame without a request keeps the OS cursor, except for the stock Grab / Grabbing /
//! ZoomIn / ZoomOut cursors, which are swapped for ours so they look the same on every platform.
//!
//! The editing tools over the tracks show their own button icon ([`Cursor::ToolIcon`]). The other
//! pointers are original vector shapes on a 24×24 grid (no image files, no third-party or Adobe
//! artwork; see `AGENTS.md` §1): a black silhouette with a white halo, readable on any picture,
//! described by a few points; `design` holds them and their hot spot (the grid point that lands on
//! the pointer).

use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, Context, CursorIcon, Id, LayerId, Order, Painter, Pos2, Rect, Stroke, pos2, vec2};

use crate::icons::{self, Icon};

/// The grid the shapes are drawn on, and the size it is drawn at (points).
const GRID: f32 = 24.0;
const SIZE: f32 = 22.0;
/// Stroke widths (points) of the white halo and of the black ink on top of it.
const HALO: f32 = 3.4;
const INK: f32 = 1.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cursor {
    /// An editing tool over the tracks: the tool button's own icon, centred on the pointer.
    ToolIcon(Icon),
    /// Trim an edge: a bracket on the clip's side of the cut and an arrow away from it.
    TrimIn,
    TrimOut,
    /// Ripple trim: the same with a second chevron, since the clips after it follow.
    RippleIn,
    RippleOut,
    /// Roll the cut between two clips.
    Roll,
    /// Rate Stretch: both ends of a clip pulled apart.
    RateStretch,
    /// Hand tool and drags of things that are picked up.
    HandOpen,
    HandClosed,
    ZoomIn,
    ZoomOut,
    /// Pen: a nib; its tip is the hot spot.
    Pen,
    /// Eyedropper: a pipette; its tip is the hot spot.
    Eyedropper,
    /// An audio clip's Volume line.
    Volume,
}

impl Cursor {
    /// Every drawn shape (the tool icons are the tools' own).
    pub const SHAPES: [Cursor; 13] = [
        Cursor::TrimIn,
        Cursor::TrimOut,
        Cursor::RippleIn,
        Cursor::RippleOut,
        Cursor::Roll,
        Cursor::RateStretch,
        Cursor::HandOpen,
        Cursor::HandClosed,
        Cursor::ZoomIn,
        Cursor::ZoomOut,
        Cursor::Pen,
        Cursor::Eyedropper,
        Cursor::Volume,
    ];
}

type Pt = (f32, f32);

/// One piece of a shape. Polygons must be convex (egui fills them as such): a concave shape is a
/// union of convex pieces.
enum Prim {
    Poly(&'static [Pt]),
    Line(&'static [Pt]),
    Ring(f32, f32, f32),
}
use Prim::{Line, Poly, Ring};

// the clip is to the right of an In edge: a "[" with its arrow pointing away from the clip
const TRIM: &[Prim] = &[Line(&[(14.0, 4.5), (11.0, 4.5), (11.0, 19.5), (14.0, 19.5)]), Poly(&[(3.0, 12.0), (9.0, 7.5), (9.0, 16.5)])];
const RIPPLE: &[Prim] = &[
    Line(&[(14.0, 4.5), (11.0, 4.5), (11.0, 19.5), (14.0, 19.5)]),
    Line(&[(8.5, 7.5), (4.5, 12.0), (8.5, 16.5)]),
    Line(&[(5.0, 7.5), (1.5, 12.0), (5.0, 16.5)]),
];
const ROLL: &[Prim] = &[
    Line(&[(10.0, 5.0), (10.0, 19.0)]),
    Line(&[(14.0, 5.0), (14.0, 19.0)]),
    Poly(&[(2.0, 12.0), (8.0, 7.5), (8.0, 16.5)]),
    Poly(&[(22.0, 12.0), (16.0, 7.5), (16.0, 16.5)]),
];
const STRETCH: &[Prim] = &[
    Line(&[(3.0, 5.0), (3.0, 19.0)]),
    Line(&[(21.0, 5.0), (21.0, 19.0)]),
    Line(&[(5.0, 12.0), (19.0, 12.0)]),
    Poly(&[(6.0, 12.0), (11.0, 8.0), (11.0, 16.0)]),
    Poly(&[(18.0, 12.0), (13.0, 8.0), (13.0, 16.0)]),
];
// a palm of six sides, four fingers and a thumb
const HAND_OPEN: &[Prim] = &[
    Poly(&[(8.0, 11.0), (16.0, 11.0), (16.0, 18.0), (14.0, 21.5), (10.0, 21.5), (8.0, 18.0)]),
    Poly(&[(8.0, 5.0), (9.9, 5.0), (9.9, 11.5), (8.0, 11.5)]),
    Poly(&[(10.3, 3.0), (12.2, 3.0), (12.2, 11.5), (10.3, 11.5)]),
    Poly(&[(12.6, 3.5), (14.5, 3.5), (14.5, 11.5), (12.6, 11.5)]),
    Poly(&[(14.9, 5.5), (16.8, 5.5), (16.8, 12.5), (14.9, 12.5)]),
    Poly(&[(4.5, 11.0), (6.5, 10.0), (9.5, 14.5), (8.0, 16.5)]),
];
const HAND_CLOSED: &[Prim] = &[
    Poly(&[(7.5, 11.0), (16.5, 11.0), (16.5, 18.0), (14.0, 21.5), (10.0, 21.5), (7.5, 18.0)]),
    Poly(&[(8.0, 8.5), (10.0, 8.5), (10.0, 11.5), (8.0, 11.5)]),
    Poly(&[(10.4, 8.0), (12.4, 8.0), (12.4, 11.5), (10.4, 11.5)]),
    Poly(&[(12.8, 8.0), (14.8, 8.0), (14.8, 11.5), (12.8, 11.5)]),
    Poly(&[(15.2, 8.5), (17.0, 8.5), (17.0, 12.0), (15.2, 12.0)]),
    Poly(&[(5.5, 13.0), (9.0, 12.0), (9.0, 16.5), (6.5, 16.0)]),
];
const ZOOM_IN: &[Prim] = &[Ring(10.0, 10.0, 6.5), Line(&[(14.8, 14.8), (21.5, 21.5)]), Line(&[(6.8, 10.0), (13.2, 10.0)]), Line(&[(10.0, 6.8), (10.0, 13.2)])];
const ZOOM_OUT: &[Prim] = &[Ring(10.0, 10.0, 6.5), Line(&[(14.8, 14.8), (21.5, 21.5)]), Line(&[(6.8, 10.0), (13.2, 10.0)])];
// a nib: the tip at the bottom left, a slit and a round vent
const PEN: &[Prim] = &[
    Poly(&[(3.0, 21.0), (5.5, 13.0), (14.0, 4.5), (19.5, 10.0), (11.0, 18.5)]),
    Line(&[(4.5, 19.5), (12.0, 12.0)]),
    Poly(&[(14.5, 3.0), (17.0, 2.0), (22.0, 7.0), (21.0, 9.5)]),
];
// a pipette: a bulb at the top right, a tube down to the tip at the bottom left
const EYEDROPPER: &[Prim] = &[
    Poly(&[(14.0, 7.5), (17.5, 4.0), (20.0, 4.0), (20.0, 6.5), (16.5, 10.0)]),
    Line(&[(11.5, 9.5), (14.5, 12.5)]),
    Line(&[(13.0, 11.0), (5.0, 19.0)]),
    Line(&[(5.0, 19.0), (3.0, 21.0)]),
];
const VOLUME: &[Prim] = &[Line(&[(12.0, 7.0), (12.0, 17.0)]), Poly(&[(12.0, 2.0), (7.5, 8.0), (16.5, 8.0)]), Poly(&[(12.0, 22.0), (7.5, 16.0), (16.5, 16.0)])];

/// A cursor's pieces, its hot spot on the grid, and whether the grid is mirrored left to right (the
/// Out edge of a trim is the In edge's shape flipped). A tool icon has no pieces of its own.
fn design(c: Cursor) -> (&'static [Prim], Pt, bool) {
    let mid = (12.0, 12.0);
    match c {
        Cursor::ToolIcon(_) => (&[], mid, false),
        Cursor::TrimIn => (TRIM, (11.0, 12.0), false),
        Cursor::TrimOut => (TRIM, (11.0, 12.0), true),
        Cursor::RippleIn => (RIPPLE, (11.0, 12.0), false),
        Cursor::RippleOut => (RIPPLE, (11.0, 12.0), true),
        Cursor::Roll => (ROLL, mid, false),
        Cursor::RateStretch => (STRETCH, mid, false),
        Cursor::HandOpen => (HAND_OPEN, (12.0, 13.0), false),
        Cursor::HandClosed => (HAND_CLOSED, (12.0, 14.0), false),
        Cursor::ZoomIn => (ZOOM_IN, (10.0, 10.0), false),
        Cursor::ZoomOut => (ZOOM_OUT, (10.0, 10.0), false),
        Cursor::Pen => (PEN, (3.0, 21.0), false),
        Cursor::Eyedropper => (EYEDROPPER, (3.0, 21.0), false),
        Cursor::Volume => (VOLUME, mid, false),
    }
}

/// The stock cursors we draw ourselves.
fn from_icon(icon: CursorIcon) -> Option<Cursor> {
    match icon {
        CursorIcon::Grab => Some(Cursor::HandOpen),
        CursorIcon::Grabbing => Some(Cursor::HandClosed),
        CursorIcon::ZoomIn => Some(Cursor::ZoomIn),
        CursorIcon::ZoomOut => Some(Cursor::ZoomOut),
        _ => None,
    }
}

fn request_id() -> Id {
    Id::new("filmcraft-cursor-request")
}

fn shown_id() -> Id {
    Id::new("filmcraft-cursor-shown")
}

/// Ask for `cursor` at the pointer for this frame (the last asker wins). The OS cursor stays the
/// fallback until [`finish`] runs, so a panel can still set one with `ctx.set_cursor_icon`.
pub fn request(ctx: &Context, cursor: Cursor) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(request_id(), (pass, cursor)));
}

/// End of the frame: swap the OS cursor for the requested one (or for the stock one it replaces) and
/// paint it at the pointer. Nothing changes while the pointer is outside the window.
pub fn finish(ctx: &Context) {
    let pass = ctx.cumulative_pass_nr();
    let asked = ctx.data(|d| d.get_temp::<(u64, Cursor)>(request_id())).filter(|(p, _)| *p == pass).map(|(_, c)| c);
    let drawn = asked.or_else(|| from_icon(ctx.output(|o| o.cursor_icon))).zip(ctx.input(|i| i.pointer.hover_pos()));
    if let Some((cursor, at)) = drawn {
        ctx.set_cursor_icon(CursorIcon::None);
        paint(&ctx.layer_painter(LayerId::new(Order::Debug, Id::new("filmcraft-cursor"))), at, cursor);
    }
    ctx.data_mut(|d| d.insert_temp(shown_id(), drawn.map(|(c, _)| c)));
}

/// The cursor the last frame painted, `None` where the OS cursor showed.
pub fn shown(ctx: &Context) -> Option<Cursor> {
    ctx.data(|d| d.get_temp::<Option<Cursor>>(shown_id())).flatten()
}

/// Paint `cursor` with its hot spot at `at`: every halo first, then the ink, so neighbouring pieces
/// never cover each other's outline.
pub fn paint(painter: &Painter, at: Pos2, cursor: Cursor) {
    if let Cursor::ToolIcon(icon) = cursor {
        // outlined so it reads over clips of any colour and theme
        let r = Rect::from_center_size(at, vec2(20.0, 20.0));
        for d in [vec2(-1.0, 0.0), vec2(1.0, 0.0), vec2(0.0, -1.0), vec2(0.0, 1.0)] {
            icons::paint(painter, r.translate(d), icon, Color32::BLACK);
        }
        icons::paint(painter, r, icon, Color32::WHITE);
        return;
    }
    let (prims, hot, mirror) = design(cursor);
    let s = SIZE / GRID;
    let x = |x: f32| if mirror { GRID - x } else { x };
    let map = |(px, py): Pt| pos2(at.x + (x(px) - x(hot.0)) * s, at.y + (py - hot.1) * s);
    let pts = |v: &[Pt]| -> Vec<Pos2> { v.iter().map(|p| map(*p)).collect() };
    for (halo, colour, width) in [(true, Color32::WHITE, HALO), (false, Color32::BLACK, INK)] {
        for prim in prims {
            match prim {
                Prim::Poly(v) => {
                    let stroke = if halo { Stroke::new(width, colour) } else { Stroke::NONE };
                    painter.add(PathShape::convex_polygon(pts(v), colour, stroke));
                }
                Prim::Line(v) => {
                    painter.add(PathShape::line(pts(v), PathStroke::new(width, colour)));
                }
                Prim::Ring(cx, cy, r) => {
                    painter.circle_stroke(map((*cx, *cy)), r * s, Stroke::new(width, colour));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(p: Pt) -> bool {
        (0.0..=GRID).contains(&p.0) && (0.0..=GRID).contains(&p.1)
    }

    #[test]
    fn every_shape_is_drawn_inside_its_grid_with_a_hot_spot_on_it() {
        for c in Cursor::SHAPES {
            let (prims, hot, _) = design(c);
            assert!(!prims.is_empty(), "{c:?} has no shape");
            assert!(inside(hot), "{c:?} hot spot {hot:?}");
            for p in prims {
                match p {
                    Prim::Poly(v) | Prim::Line(v) => {
                        assert!(v.len() >= 2, "{c:?}");
                        assert!(v.iter().all(|q| inside(*q)), "{c:?} has a point off the grid");
                    }
                    Prim::Ring(x, y, r) => assert!(inside((x - r, y - r)) && inside((x + r, y + r)), "{c:?} ring off the grid"),
                }
            }
        }
    }

    #[test]
    fn polygons_are_convex() {
        // egui fills polygons as convex: every turn must go the same way
        for c in Cursor::SHAPES {
            for p in design(c).0 {
                let Prim::Poly(v) = p else { continue };
                let n = v.len();
                let turns: Vec<f32> = (0..n)
                    .map(|i| {
                        let (a, b, d) = (v[i], v[(i + 1) % n], v[(i + 2) % n]);
                        (b.0 - a.0) * (d.1 - b.1) - (b.1 - a.1) * (d.0 - b.0)
                    })
                    .collect();
                assert!(turns.iter().all(|t| *t >= -1e-4) || turns.iter().all(|t| *t <= 1e-4), "{c:?} has a concave polygon {v:?}");
            }
        }
    }

    #[test]
    fn the_out_edge_is_the_in_edge_mirrored() {
        assert!(!design(Cursor::TrimIn).2);
        assert!(design(Cursor::TrimOut).2);
        assert!(std::ptr::eq(design(Cursor::TrimIn).0, design(Cursor::TrimOut).0));
    }

    #[test]
    fn stock_grab_and_zoom_cursors_are_replaced_and_the_rest_are_left_alone() {
        assert_eq!(from_icon(CursorIcon::Grab), Some(Cursor::HandOpen));
        assert_eq!(from_icon(CursorIcon::Grabbing), Some(Cursor::HandClosed));
        assert_eq!(from_icon(CursorIcon::ZoomIn), Some(Cursor::ZoomIn));
        assert_eq!(from_icon(CursorIcon::Default), None);
        assert_eq!(from_icon(CursorIcon::Text), None);
        assert_eq!(from_icon(CursorIcon::ResizeColumn), None);
    }

    fn frame(ctx: &Context, pointer: Option<Pos2>, mut ui: impl FnMut(&Context)) -> egui::FullOutput {
        let mut input = egui::RawInput::default();
        input.events.push(match pointer {
            Some(p) => egui::Event::PointerMoved(p),
            None => egui::Event::PointerGone,
        });
        let mut out = ctx.run_ui(input, |ui_| ui(ui_.ctx()));
        // a dropped output with texture changes nobody applied panics
        out.textures_delta.clear();
        out
    }

    #[test]
    fn a_requested_cursor_hides_the_os_cursor_and_is_painted_at_the_pointer() {
        let ctx = Context::default();
        for cursor in [Cursor::TrimIn, Cursor::ToolIcon(Icon::Razor)] {
            for _ in 0..2 {
                let out = frame(&ctx, Some(pos2(60.0, 40.0)), |ctx| {
                    ctx.set_cursor_icon(CursorIcon::ResizeColumn);
                    request(ctx, cursor);
                    finish(ctx);
                });
                assert_eq!(out.platform_output.cursor_icon, CursorIcon::None);
                assert!(!out.shapes.is_empty());
                assert_eq!(shown(&ctx), Some(cursor));
            }
        }
    }

    #[test]
    fn nothing_changes_without_a_request_or_outside_the_window() {
        let ctx = Context::default();
        let out = frame(&ctx, Some(pos2(60.0, 40.0)), |ctx| {
            ctx.set_cursor_icon(CursorIcon::ResizeColumn);
            finish(ctx);
        });
        assert_eq!(out.platform_output.cursor_icon, CursorIcon::ResizeColumn);
        assert_eq!(shown(&ctx), None);
        let out = frame(&ctx, None, |ctx| {
            request(ctx, Cursor::Pen);
            finish(ctx);
        });
        assert_ne!(out.platform_output.cursor_icon, CursorIcon::None);
        assert_eq!(shown(&ctx), None);
    }
}
