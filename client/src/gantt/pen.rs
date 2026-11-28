//! A canvas context that skips setting what is already set, saving a JavaScript call and a font
//! parse. Its methods shadow the context's own; the rest are reached through `Deref`.

// modules {{{
use std::{cell::RefCell, ops::Deref};

use wasm_bindgen::{JsValue, intern};
use web_sys::{CanvasGradient, CanvasRenderingContext2d};
// }}}

// changed {{{
fn changed<T: PartialEq + Clone>(field: &mut Option<T>, value: &T) -> bool {
    if field.as_ref() == Some(value) {
        return false;
    }
    *field = Some(value.clone());
    true
}
// }}}

pub struct Pen {
    ctx: CanvasRenderingContext2d,
    state: RefCell<State>,
    saved: RefCell<Vec<State>>,
    /// Text widths by font and text
    widths: RefCell<std::collections::HashMap<(String, String), f64>>,
}

// Pen supporting types {{{
#[derive(Clone, Default)]
struct State {
    fill: Option<String>,
    stroke: Option<String>,
    font: Option<String>,
    baseline: Option<String>,
    align: Option<String>,
    width: Option<f64>,
}
// }}}

// impl Pen {{{
impl Deref for Pen {
    type Target = CanvasRenderingContext2d;
    fn deref(&self) -> &CanvasRenderingContext2d {
        &self.ctx
    }
}

impl Pen {
    /// The context's settings must be the defaults, as after the canvas is resized
    pub fn new(ctx: CanvasRenderingContext2d) -> Self {
        Self {
            ctx,
            state: RefCell::default(),
            saved: RefCell::default(),
            widths: RefCell::default(),
        }
    }

    pub fn set_fill_style_str(&self, colour: &str) {
        if changed(&mut self.state.borrow_mut().fill, &colour.to_string()) {
            self.ctx.set_fill_style_str(intern(colour));
        }
    }

    pub fn set_stroke_style_str(&self, colour: &str) {
        if changed(&mut self.state.borrow_mut().stroke, &colour.to_string()) {
            self.ctx.set_stroke_style_str(intern(colour));
        }
    }

    pub fn set_stroke_style_canvas_gradient(&self, gradient: &CanvasGradient) {
        self.state.borrow_mut().stroke = None;
        self.ctx.set_stroke_style_canvas_gradient(gradient);
    }

    pub fn set_font(&self, font: &str) {
        if changed(&mut self.state.borrow_mut().font, &font.to_string()) {
            self.ctx.set_font(intern(font));
        }
    }

    pub fn set_text_baseline(&self, baseline: &str) {
        if changed(&mut self.state.borrow_mut().baseline, &baseline.to_string()) {
            self.ctx.set_text_baseline(intern(baseline));
        }
    }

    pub fn set_text_align(&self, align: &str) {
        if changed(&mut self.state.borrow_mut().align, &align.to_string()) {
            self.ctx.set_text_align(intern(align));
        }
    }

    pub fn set_line_width(&self, width: f64) {
        if changed(&mut self.state.borrow_mut().width, &width) {
            self.ctx.set_line_width(width);
        }
    }

    pub fn fill_text(&self, text: &str, x: f64, y: f64) -> Result<(), JsValue> {
        self.ctx.fill_text(intern(text), x, y)
    }

    pub fn text_width(&self, text: &str) -> f64 {
        let font = self.state.borrow().font.clone().unwrap_or_default();
        let key = (font, text.to_string());
        if let Some(width) = self.widths.borrow().get(&key) {
            return *width;
        }
        let width = self.ctx.measure_text(text).map_or(0.0, |m| m.width());
        let mut widths = self.widths.borrow_mut();
        if widths.len() > 5000 {
            widths.clear();
        }
        widths.insert(key, width);
        width
    }

    pub fn save(&self) {
        self.saved.borrow_mut().push(self.state.borrow().clone());
        self.ctx.save();
    }

    pub fn restore(&self) {
        if let Some(state) = self.saved.borrow_mut().pop() {
            *self.state.borrow_mut() = state;
        }
        self.ctx.restore();
    }
}
// }}}
