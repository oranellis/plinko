use skia_safe::{
    Canvas, Color, Paint, PaintStyle, Path, PathBuilder, RRect, Rect,
};

use super::layout::*;
use crate::pages::PageId;

pub fn draw_toolbar(canvas: &Canvas, width: f32, active_page: PageId, hovered_button: Option<usize>, icon_paths: &[Path; BUTTON_COUNT]) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from(TOOLBAR_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rect(
        Rect::from_xywh(0.0, 0.0, width, TOOLBAR_HEIGHT),
        &paint,
    );

    paint.set_color(Color::from(TOOLBAR_BORDER));
    canvas.draw_rect(
        Rect::from_xywh(0.0, TOOLBAR_HEIGHT - 1.0, width, 1.0),
        &paint,
    );

    let start_x = BUTTON_MARGIN;
    for i in 0..BUTTON_COUNT {
        let bx = start_x + i as f32 * (BUTTON_SIZE + BUTTON_MARGIN);
        let by = (TOOLBAR_HEIGHT - BUTTON_SIZE) / 2.0;
        let hovered = hovered_button == Some(i);
        let active = match i {
            0 => active_page == PageId::Daily,
            1 => active_page == PageId::Planning,
            2 => active_page == PageId::Settings,
            _ => false,
        };
        draw_button(canvas, icon_paths, i, bx, by, BUTTON_SIZE, hovered, active);
    }
}

fn draw_button(
    canvas: &Canvas,
    icon_paths: &[Path; BUTTON_COUNT],
    index: usize,
    x: f32,
    y: f32,
    size: f32,
    hovered: bool,
    active: bool,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    if active {
        paint.set_color(Color::from(BUTTON_ACTIVE_BG));
        paint.set_style(PaintStyle::Fill);
        let rrect = RRect::new_rect_xy(Rect::from_xywh(x, y, size, size), 4.0, 4.0);
        canvas.draw_rrect(rrect, &paint);
    } else if hovered {
        paint.set_color(Color::from(BUTTON_HOVER_BG));
        paint.set_style(PaintStyle::Fill);
        let rrect = RRect::new_rect_xy(Rect::from_xywh(x, y, size, size), 4.0, 4.0);
        canvas.draw_rrect(rrect, &paint);
    }

    let icon_color = if active { ICON_ACTIVE_COLOR } else { ICON_COLOR };
    paint.set_color(Color::from(icon_color));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);

    canvas.save();
    canvas.translate((x + BUTTON_PADDING, y + BUTTON_PADDING));
    canvas.draw_path(&icon_paths[index], &paint);
    canvas.restore();
}

pub fn hit_test_button(x: f32, y: f32) -> Option<usize> {
    if y < BUTTON_MARGIN || y > TOOLBAR_HEIGHT - BUTTON_MARGIN {
        return None;
    }
    let start_x = BUTTON_MARGIN;
    for i in 0..BUTTON_COUNT {
        let bx = start_x + i as f32 * (BUTTON_SIZE + BUTTON_MARGIN);
        if x >= bx
            && x <= bx + BUTTON_SIZE
            && y >= BUTTON_MARGIN
            && y <= BUTTON_MARGIN + BUTTON_SIZE
        {
            return Some(i);
        }
    }
    None
}

// --- Icon builders ---

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
    pb.add_arc(&oval, 0.0, 360.0);
    pb.detach()
}

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
        pb.add_arc(&oval, 0.0, 360.0);
    }
    pb.detach()
}

pub fn build_icon_undo(w: f32, h: f32) -> Path {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let r = w * 0.35;
    let mut pb = PathBuilder::new();
    let oval = Rect::from_xywh(cx - r, cy - r, 2.0 * r, 2.0 * r);
    pb.add_arc(&oval, 200.0, -250.0);
    let ax = cx - r * 200.0_f32.to_radians().cos().abs();
    let ay = cy + r * 200.0_f32.to_radians().sin().abs() * 0.3;
    let s = w * 0.18;
    pb.move_to((ax - s, ay - s * 0.3));
    pb.line_to((ax, ay));
    pb.line_to((ax + s * 0.3, ay - s));
    pb.detach()
}

pub fn build_icon_redo(w: f32, h: f32) -> Path {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let r = w * 0.35;
    let mut pb = PathBuilder::new();
    let oval = Rect::from_xywh(cx - r, cy - r, 2.0 * r, 2.0 * r);
    pb.add_arc(&oval, -20.0, 250.0);
    let angle_rad = (230.0_f32).to_radians();
    let ax = cx + r * angle_rad.cos();
    let ay = cy + r * angle_rad.sin() * 0.3;
    let s = w * 0.18;
    pb.move_to((ax + s, ay - s * 0.3));
    pb.line_to((ax, ay));
    pb.line_to((ax - s * 0.3, ay - s));
    pb.detach()
}
