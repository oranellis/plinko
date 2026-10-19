//! The chart is drawn in layers that are kept as images: what is behind the items (the days,
//! today, groups) and the items themselves. They do not change when the pointer moves, so hovering
//! only draws the lines, outlines and names over them. They are drawn bigger than the screen, so
//! scrolling just moves them until the screen is about to run off their edge.

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
    /// The size of what is drawn on them, in chart pixels, which can be less than they hold
    size: (f64, f64),
    ratio: f64,
    /// The size they were made for
    room: (f64, f64),
    /// Where the top left of what was drawn is in the chart, in pixels
    origin: (f64, f64),
    /// The names to draw over the layers, positioned in them
    pub labels: Vec<Label>,
}

// constants {{{
/// How much is drawn past each side of the screen, as a share of the screen up to a limit
const SPARE: f64 = 0.5;
const SPARE_X: f64 = 600.0;
const SPARE_Y: f64 = 400.0;
// }}}

// Layers supporting types {{{
/// What the layers were drawn from. They are drawn again when any of it changes.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    /// The screen's size in pixels
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
    /// Where the layers' top left is on the screen for the chart scrolled to `viewport`
    pub fn at(&self, viewport: Viewport) -> (f64, f64) {
        (
            self.origin.0 - viewport.scroll_x,
            self.origin.1 - viewport.scroll_y,
        )
    }

    /// Whether they still show the chart as it is and cover the screen
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
/// Draws the layers for the screen as it is now, using the images already made if they are the
/// right size
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
    // while zooming everything moves at every step, so only a little more than the screen is drawn
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
    // the chart as seen from the layers' top left
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
        links: &[],
        flash: None,
        info: &[],
        tip: None,
    };
    let mut found = Found::default();
    paint_back(&back.pen, &frame, &mut found);
    // what was drawn before on this layer is wiped, as it has no colour of its own
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
