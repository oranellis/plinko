//! Momentum for sideways wheel and touchpad scrolling.

// modules {{{
use std::{cell::Cell, rc::Rc};

use leptos::prelude::request_animation_frame;
// }}}

// constants {{{
/// Milliseconds without a wheel event that count as the gesture letting go
const RELEASE_MS: f64 = 70.0;
/// Fraction of the speed kept per 1/60 s frame
const KEPT: f64 = 0.93;
/// Pixels per millisecond below which the glide stops
const STOP: f64 = 0.03;
// }}}

// helpers {{{
/// `dt` is milliseconds since the last wheel event
fn speed(before: f64, delta: f64, dt: f64) -> f64 {
    // a long pause is a new gesture
    if dt > 100.0 {
        return 0.0;
    }
    let now = delta / dt.max(4.0);
    0.5 * now + 0.5 * before
}

fn fade(speed: f64, dt: f64) -> f64 {
    speed * KEPT.powf(dt.clamp(1.0, 50.0) / (1000.0 / 60.0))
}
// }}}

#[derive(Clone)]
pub struct Glide {
    /// Pixels per millisecond
    speed: Rc<Cell<f64>>,
    last: Rc<Cell<f64>>,
    /// The sub-pixel remainder not yet applied
    carry: Rc<Cell<f64>>,
    running: Rc<Cell<bool>>,
    /// Scrolls by whole pixels; returns false once the target is gone
    apply: Rc<dyn Fn(f64) -> bool>,
}

// impl Glide {{{
impl Glide {
    pub fn new(apply: impl Fn(f64) -> bool + 'static) -> Self {
        Self {
            speed: Rc::default(),
            last: Rc::default(),
            carry: Rc::default(),
            running: Rc::default(),
            apply: Rc::new(apply),
        }
    }

    fn move_by(&self, amount: f64) -> bool {
        let total = self.carry.get() + amount;
        let whole = total.round();
        self.carry.set(total - whole);
        whole == 0.0 || (self.apply)(whole)
    }

    pub fn push(&self, delta: f64) {
        let now = js_sys::Date::now();
        let before = self.last.get();
        self.speed.set(speed(self.speed.get(), delta, now - before));
        self.last.set(now);
        self.move_by(delta);
        if !self.running.replace(true) {
            self.frame(now);
        }
    }

    fn frame(&self, before: f64) {
        let now = js_sys::Date::now();
        let mut alive = true;
        if now - self.last.get() >= RELEASE_MS {
            let moving = self.speed.get();
            if moving.abs() < STOP {
                alive = false;
            } else {
                let dt = (now - before).clamp(1.0, 50.0);
                alive = self.move_by(moving * dt);
                self.speed.set(fade(moving, dt));
            }
        }
        if !alive {
            self.speed.set(0.0);
            self.carry.set(0.0);
            self.running.set(false);
            return;
        }
        let this = self.clone();
        request_animation_frame(move || this.frame(now));
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speed_follows_the_gesture_and_a_pause_starts_afresh() {
        let mut v: f64 = 0.0;
        for _ in 0..10 {
            v = speed(v, 16.0, 16.0);
        }
        assert!((v - 1.0).abs() < 0.01);
        assert_eq!(speed(v, 16.0, 500.0), 0.0);
    }

    #[test]
    fn it_fades_away_without_changing_direction() {
        let mut v: f64 = -1.5;
        let mut frames = 0;
        let mut travelled = 0.0;
        while v.abs() >= STOP {
            travelled += v * 16.7;
            v = fade(v, 16.7);
            frames += 1;
            assert!(v < 0.0 && frames < 300);
        }
        assert!(travelled < -150.0 && travelled > -600.0, "{travelled}");
    }
}
// }}}
