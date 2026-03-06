use skia_safe::{Path, PathBuilder, Rect};

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
        pb.add_arc(oval, 0.0, 360.0);
    }
    pb.detach()
}
