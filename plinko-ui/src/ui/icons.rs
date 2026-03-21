//! Skia path builders for the three home-screen navigation icons.
//!
//! All icons are drawn in a `w × h` bounding box starting at the origin;
//! callers translate the canvas before drawing.

use skia_safe::{Matrix, Path, PathBuilder, Rect};

/// Builds a calendar-style icon: outline rectangle, header bar, two pin
/// hangers at the top, and a filled circle representing a day.
pub fn build_icon_daily(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    // Calendar outline
    pb.move_to((0.0, h * 0.15));
    pb.line_to((w, h * 0.15));
    pb.line_to((w, h));
    pb.line_to((0.0, h));
    pb.close();
    // Header bar
    pb.move_to((0.0, h * 0.35));
    pb.line_to((w, h * 0.35));
    // Calendar hangers
    pb.move_to((w * 0.25, 0.0));
    pb.line_to((w * 0.25, h * 0.25));
    pb.move_to((w * 0.75, 0.0));
    pb.line_to((w * 0.75, h * 0.25));
    // Day dot
    let cx = w * 0.5;
    let cy = h * 0.65;
    let r = w * 0.08;
    let oval = Rect::from_xywh(cx - r, cy - r, 2.0 * r, 2.0 * r);
    pb.add_arc(oval, 0.0, 360.0);
    pb.detach()
}

/// Builds a two-column split-view icon representing the planning layout.
pub fn build_icon_planning(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    // Two columns representing split view
    let gap = w * 0.1;
    let col_w = (w - gap) / 2.0;
    // Left column
    pb.move_to((0.0, 0.0));
    pb.line_to((col_w, 0.0));
    pb.line_to((col_w, h));
    pb.line_to((0.0, h));
    pb.close();
    // Right column
    let rx = col_w + gap;
    pb.move_to((rx, 0.0));
    pb.line_to((w, 0.0));
    pb.line_to((w, h));
    pb.line_to((rx, h));
    pb.close();
    pb.detach()
}

/// Builds a plus / add icon: two perpendicular lines crossing at the centre.
pub fn build_icon_plus(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    // Vertical bar
    pb.move_to((w * 0.5, 0.0));
    pb.line_to((w * 0.5, h));
    // Horizontal bar
    pb.move_to((0.0, h * 0.5));
    pb.line_to((w, h * 0.5));
    pb.detach()
}

/// Builds a diamond icon (rotated square) used to represent a milestone.
pub fn build_icon_diamond(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    pb.move_to((w * 0.5, 0.0)); // top
    pb.line_to((w, h * 0.5)); // right
    pb.line_to((w * 0.5, h)); // bottom
    pb.line_to((0.0, h * 0.5)); // left
    pb.close();
    pb.detach()
}

/// Builds a person silhouette icon: head circle and shoulder arc.
pub fn build_icon_person(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    // Head
    let head_r = w * 0.22;
    let head_cx = w * 0.5;
    let head_cy = h * 0.3;
    pb.add_arc(
        Rect::from_xywh(
            head_cx - head_r,
            head_cy - head_r,
            2.0 * head_r,
            2.0 * head_r,
        ),
        0.0,
        360.0,
    );
    // Shoulders arc — large circle centred just below the icon
    let body_r = w * 0.42;
    let body_cx = w * 0.5;
    let body_cy = h * 1.08;
    pb.add_arc(
        Rect::from_xywh(
            body_cx - body_r,
            body_cy - body_r,
            2.0 * body_r,
            2.0 * body_r,
        ),
        205.0,
        130.0,
    );
    pb.detach()
}

/// Builds a gift-tag / price-tag icon: a rounded rectangle with a pointed
/// left side and a small string hole, like a label tied to a package.
pub fn build_icon_tag(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();

    let m = w * 0.06;
    let tip_x = m;
    let body_x = w * 0.32;
    let right = w - m;
    let top = h * 0.14;
    let bottom = h - h * 0.14;
    let cy = h * 0.5;
    let r = (bottom - top) * 0.18;

    // Outline: pointed left side → rounded rectangle
    pb.move_to((tip_x, cy));
    pb.line_to((body_x, top));
    pb.line_to((right - r, top));
    pb.quad_to((right, top), (right, top + r));
    pb.line_to((right, bottom - r));
    pb.quad_to((right, bottom), (right - r, bottom));
    pb.line_to((body_x, bottom));
    pb.close();

    // String hole near the pointed end
    let hole_cx = body_x + (right - body_x) * 0.18;
    let hole_r = (bottom - top) * 0.1;
    let oval = Rect::from_xywh(hole_cx - hole_r, cy - hole_r, hole_r * 2.0, hole_r * 2.0);
    pb.add_arc(oval, 0.0, 360.0);

    let path = pb.detach();
    let cx = w / 2.0;
    let cy = h / 2.0;
    let mut m = Matrix::new_identity();
    m.pre_translate((cx, cy));
    m.pre_rotate(135.0, None);
    m.pre_translate((-cx, -cy));
    path.with_transform(&m)
}

/// Builds a "go to today" icon: a vertical bar (today marker) with a small
/// filled circle on it, like a play-head or position indicator.
pub fn build_icon_today(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    // Vertical line centred horizontally
    let cx = w * 0.5;
    pb.move_to((cx, 0.0));
    pb.line_to((cx, h));
    // Small filled arrowhead pointing left at mid height
    let ay = h * 0.5;
    pb.move_to((cx, ay));
    pb.line_to((cx - w * 0.32, ay - h * 0.22));
    pb.line_to((cx - w * 0.32, ay + h * 0.22));
    pb.close();
    pb.detach()
}

/// Builds a stacked vertical bar chart icon representing workload allocation.
pub fn build_icon_allocation(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    let bar_w = w * 0.18;
    let gap = (w - 3.0 * bar_w) / 4.0;

    // Bar 1 (left) — tall
    let x1 = gap;
    pb.move_to((x1, h * 0.2));
    pb.line_to((x1 + bar_w, h * 0.2));
    pb.line_to((x1 + bar_w, h));
    pb.line_to((x1, h));
    pb.close();

    // Bar 2 (middle) — medium
    let x2 = 2.0 * gap + bar_w;
    pb.move_to((x2, h * 0.45));
    pb.line_to((x2 + bar_w, h * 0.45));
    pb.line_to((x2 + bar_w, h));
    pb.line_to((x2, h));
    pb.close();

    // Bar 3 (right) — short
    let x3 = 3.0 * gap + 2.0 * bar_w;
    pb.move_to((x3, h * 0.65));
    pb.line_to((x3 + bar_w, h * 0.65));
    pb.line_to((x3 + bar_w, h));
    pb.line_to((x3, h));
    pb.close();

    pb.detach()
}

/// Builds a calendar grid icon with column and row dividers, representing
/// the calendar overrides editing view.
pub fn build_icon_calendar_edit(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    // Calendar outline
    pb.move_to((0.0, h * 0.15));
    pb.line_to((w, h * 0.15));
    pb.line_to((w, h));
    pb.line_to((0.0, h));
    pb.close();
    // Header bar
    pb.move_to((0.0, h * 0.35));
    pb.line_to((w, h * 0.35));
    // Calendar hangers
    pb.move_to((w * 0.25, 0.0));
    pb.line_to((w * 0.25, h * 0.25));
    pb.move_to((w * 0.75, 0.0));
    pb.line_to((w * 0.75, h * 0.25));
    // Vertical grid lines inside calendar body
    pb.move_to((w * 0.33, h * 0.35));
    pb.line_to((w * 0.33, h));
    pb.move_to((w * 0.67, h * 0.35));
    pb.line_to((w * 0.67, h));
    // Horizontal grid lines inside calendar body
    pb.move_to((0.0, h * 0.57));
    pb.line_to((w, h * 0.57));
    pb.move_to((0.0, h * 0.79));
    pb.line_to((w, h * 0.79));
    pb.detach()
}

/// Builds a three-line slider icon (horizontal rules with circular knobs)
/// representing settings / configuration.
pub fn build_icon_settings(w: f32, h: f32) -> Path {
    let mut pb = PathBuilder::new();
    // Three horizontal lines with knobs (slider-settings style)
    for i in 0..3 {
        let y = h * 0.2 + i as f32 * h * 0.3;
        pb.move_to((0.0, y));
        pb.line_to((w, y));
        let knob_x = match i {
            0 => w * 0.7,
            1 => w * 0.35,
            _ => w * 0.55,
        };
        let r = w * 0.08;
        let oval = Rect::from_xywh(knob_x - r, y - r, 2.0 * r, 2.0 * r);
        pb.add_arc(oval, 0.0, 360.0);
    }
    pb.detach()
}
