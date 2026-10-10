//! A little momentum for scrolling by wheel: what the wheel asks for is not applied at once but a
//! share of what is left each frame, so a notch eases in and out instead of jumping.

use std::{cell::Cell, rc::Rc};

use leptos::prelude::request_animation_frame;

/// What is left of the distance after a frame of 1/60th of a second
const KEPT: f64 = 0.8;
/// Under this many pixels the rest is taken in one go
const SETTLED: f64 = 0.5;

/// How much of what is left to move in a frame that took `dt` milliseconds
fn share(left: f64, dt: f64) -> f64 {
    let part = left * (1.0 - KEPT.powf(dt.clamp(1.0, 50.0) / (1000.0 / 60.0)));
    if part.abs() < SETTLED { left } else { part }
}

#[derive(Clone)]
pub struct Glide {
    left: Rc<Cell<f64>>,
    running: Rc<Cell<bool>>,
    /// Moves the thing scrolled by a distance, and says whether it is still there to move
    apply: Rc<dyn Fn(f64) -> bool>,
}

impl Glide {
    pub fn new(apply: impl Fn(f64) -> bool + 'static) -> Self {
        Self {
            left: Rc::default(),
            running: Rc::default(),
            apply: Rc::new(apply),
        }
    }

    /// Asks for `delta` more pixels of scrolling
    pub fn push(&self, delta: f64) {
        self.left.set(self.left.get() + delta);
        if !self.running.replace(true) {
            self.frame(js_sys::Date::now());
        }
    }

    fn frame(&self, before: f64) {
        let now = js_sys::Date::now();
        let left = self.left.get();
        let part = share(left, now - before);
        self.left.set(left - part);
        let alive = part == 0.0 || (self.apply)(part);
        if !alive || self.left.get() == 0.0 {
            self.left.set(0.0);
            self.running.set(false);
            return;
        }
        let this = self.clone();
        request_animation_frame(move || this.frame(now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_eases_out_and_arrives_exactly() {
        let mut left = 100.0;
        let mut moved = 0.0;
        let mut steps = 0;
        while left != 0.0 {
            let part = share(left, 16.7);
            assert!(part > 0.0 && part <= left);
            moved += part;
            left -= part;
            steps += 1;
            assert!(steps < 200, "it should settle");
        }
        assert!((moved - 100.0).abs() < 1e-9);
        // the first frame moves the most
        assert!(share(100.0, 16.7) > share(50.0, 16.7));
        // and a negative distance goes the other way
        assert!(share(-100.0, 16.7) < 0.0);
    }
}
