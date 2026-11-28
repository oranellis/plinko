//! Cached background and item layers, so hovering only redraws the overlay. They are drawn larger
//! than the screen, so scrolling just moves them until the screen nears their edge.

// modules {{{
use std::rc::Rc;

use chrono::NaiveDate;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::gantt::{
    draw::{Found, Frame, Label, paint_back, paint_nodes},
    layout::Viewport,
    pen::Pen,
    scene::Scene,
};
// }}}

pub struct Layers {
    back: Image,
    nodes: Image,
    key: Key,
    /// The drawn area in chart pixels, which can be less than the canvas holds
    size: (f64, f64),
    ratio: f64,
    /// The size the canvases were allocated for
    room: (f64, f64),
    /// The drawn area's top left in chart pixels
    origin: (f64, f64),
    /// Positioned relative to the layers
    pub labels: Vec<Label>,
}

// constants {{{
/// Margin drawn past each side of the screen, as a share of it, capped
const SPARE: f64 = 0.5;
const SPARE_X: f64 = 600.0;
const SPARE_Y: f64 = 400.0;
// }}}

// Layers supporting types {{{
/// The layers are redrawn when any of this changes
#[derive(Clone, Copy, PartialEq)]
struct Key {
    /// The screen's size in device pixels
    pixels: (u32, u32),
    day_width: f64,
    generation: u64,
    today: NaiveDate,
}

struct Image {
    canvas: HtmlCanvasElement,
    pen: Rc<Pen>,
}
// }}}

// impl Image {{{
impl Image {
    fn new(width: u32, height: u32, ratio: f64) -> Option<Self> {
        let canvas = web_sys::window()?
            .document()?
            .create_element("canvas")
            .ok()?
            .dyn_into::<HtmlCanvasElement>()
            .ok()?;
        canvas.set_width(width);
        canvas.set_height(height);
        let ctx = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<CanvasRenderingContext2d>()
            .ok()?;
        let _ = ctx.set_transform(ratio, 0.0, 0.0, ratio, 0.0, 0.0);
        Some(Self {
            canvas,
            pen: Rc::new(Pen::new(ctx)),
        })
    }
}
// }}}

// impl Layers {{{
impl Layers {
    /// The layers' top left on screen
    pub fn at(&self, viewport: Viewport) -> (f64, f64) {
        (
            self.origin.0 - viewport.scroll_x,
            self.origin.1 - viewport.scroll_y,
        )
    }

    /// Whether they are up to date and still cover the screen
    pub fn serve(
        &self,
        viewport: Viewport,
        (width, height): (f64, f64),
        ratio: f64,
        generation: u64,
        today: NaiveDate,
    ) -> bool {
        let (x, y) = self.at(viewport);
        self.key == key((width, height), ratio, viewport, generation, today)
            && x <= 0.0
            && y <= 0.0
            && x + self.size.0 >= width
            && y + self.size.1 >= height
    }

    pub fn blit(&self, ctx: &Pen, at: (f64, f64), back: bool) {
        let image = if back { &self.back } else { &self.nodes };
        let _ = ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            &image.canvas,
            0.0,
            0.0,
            self.size.0 * self.ratio,
            self.size.1 * self.ratio,
            at.0,
            at.1,
            self.size.0,
            self.size.1,
        );
    }
}
// }}}

// key {{{
fn key(
    (width, height): (f64, f64),
    ratio: f64,
    viewport: Viewport,
    generation: u64,
    today: NaiveDate,
) -> Key {
    Key {
        pixels: ((width * ratio) as u32, (height * ratio) as u32),
        day_width: viewport.day_width,
        generation,
        today,
    }
}
// }}}

// render {{{
/// Reuses the old canvases when they are the right size
pub fn render(
    old: Option<Layers>,
    (width, height): (f64, f64),
    ratio: f64,
    viewport: Viewport,
    scene: &Scene,
    generation: u64,
    today: NaiveDate,
) -> Option<(Layers, Found)> {
    let room = (
        width + 2.0 * (width * SPARE).min(SPARE_X).round(),
        height + 2.0 * (height * SPARE).min(SPARE_Y).round(),
    );
    // while zooming every step redraws anyway, so only a small margin is drawn
    let zooming = old
        .as_ref()
        .is_some_and(|l| l.key.day_width != viewport.day_width);
    let (spare_x, spare_y) = if zooming {
        ((width * 0.05).round(), (height * 0.05).round())
    } else {
        ((room.0 - width) / 2.0, (room.1 - height) / 2.0)
    };
    let size = (width + 2.0 * spare_x, height + 2.0 * spare_y);
    let pixels = ((room.0 * ratio) as u32, (room.1 * ratio) as u32);
    let reusable = old.filter(|l| {
        l.room == room && l.back.canvas.width() == pixels.0 && l.back.canvas.height() == pixels.1
    });
    let (back, nodes) = match reusable {
        Some(l) => (l.back, l.nodes),
        None => (
            Image::new(pixels.0, pixels.1, ratio)?,
            Image::new(pixels.0, pixels.1, ratio)?,
        ),
    };
    let origin = (viewport.scroll_x - spare_x, viewport.scroll_y - spare_y);
    let window = Viewport {
        scroll_x: origin.0,
        scroll_y: origin.1,
        ..viewport
    };
    let frame = Frame {
        width: size.0,
        height: size.1,
        top: 0.0,
        viewport: window,
        scene,
        today,
        hover: None,
        selected: None,
        staged: None,
        nameless: None,
        ghosted: None,
        links: &[],
        flash: None,
        tip: None,
    };
    let mut found = Found::default();
    paint_back(&back.pen, &frame, &mut found);
    // this layer is transparent, so the old drawing must be wiped
    nodes.pen.clear_rect(0.0, 0.0, room.0, room.1);
    paint_nodes(&nodes.pen, &frame, &mut found);
    let labels = std::mem::take(&mut found.labels);
    Some((
        Layers {
            back,
            nodes,
            key: key((width, height), ratio, viewport, generation, today),
            size,
            ratio,
            room,
            origin,
            labels,
        },
        found,
    ))
}
// }}}
