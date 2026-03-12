//! Floating window (modal overlay) infrastructure.
//!
//! Provides the [`FloatingWindow`] trait, [`FloatingWindowOutcome`] result
//! type, and [`FloatingWindowManager`] stack for layered modal dialogs.

use skia_safe::{Canvas, Color, Paint, Rect};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::data::Plan;
use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::layout::OVERLAY_DARK;

/// Outcome returned by every [`FloatingWindow`] event handler.
pub struct FloatingWindowOutcome {
    pub dirty: DirtyRegion,
    /// When `true`, the manager pops this window after the event.
    pub close: bool,
}

impl Default for FloatingWindowOutcome {
    fn default() -> Self {
        Self {
            dirty: DirtyRegion::None,
            close: false,
        }
    }
}

impl FloatingWindowOutcome {
    /// Close this window and request a full redraw.
    pub fn close() -> Self {
        Self {
            dirty: DirtyRegion::All,
            close: true,
        }
    }

    /// Keep the window open, request the given dirty region.
    pub fn dirty(region: DirtyRegion) -> Self {
        Self {
            dirty: region,
            close: false,
        }
    }
}

/// A modal overlay that sits above page content and below the back button.
pub trait FloatingWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan);

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        plan: &Plan,
    ) -> FloatingWindowOutcome;

    #[allow(clippy::too_many_arguments)]
    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        modifiers: &Modifiers,
        plan: &Plan,
        sender: &PlanRequestSender,
        cache: &RenderCache,
    ) -> FloatingWindowOutcome;

    /// Default: close on Escape, ignore everything else.
    #[allow(clippy::too_many_arguments)]
    fn on_key_input(
        &mut self,
        key: &Key,
        _sender: &PlanRequestSender,
        _width: f32,
        _height: f32,
        _plan: &Plan,
        _cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if *key == Key::Named(NamedKey::Escape) {
            FloatingWindowOutcome::close()
        } else {
            FloatingWindowOutcome::default()
        }
    }

    /// Called on mouse-wheel / trackpad scroll while this window is topmost.
    /// `delta_y` is positive when scrolling up (content moves down).
    fn on_scroll(
        &mut self,
        _delta_y: f32,
        _plan: &Plan,
        _width: f32,
        _height: f32,
    ) -> FloatingWindowOutcome {
        FloatingWindowOutcome::default()
    }

    /// Called after `on_mouse_input` to check whether this window wants to push
    /// a child window onto the stack.  Implementations set an internal flag on
    /// click and return the new window here, consuming the flag.
    fn take_open_request(&mut self) -> Option<Box<dyn FloatingWindow>> {
        None
    }

    fn reset_hover(&mut self) {}
}

/// Manages a stack of [`FloatingWindow`]s, routing events to the topmost one.
pub struct FloatingWindowManager {
    stack: Vec<Box<dyn FloatingWindow>>,
}

impl FloatingWindowManager {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn is_open(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Push a new window onto the stack. Resets hover on the previous top.
    pub fn push(&mut self, window: Box<dyn FloatingWindow>) {
        if let Some(top) = self.stack.last_mut() {
            top.reset_hover();
        }
        self.stack.push(window);
    }

    /// Renders a dim backdrop then all windows bottom-to-top.
    pub fn render(
        &self,
        canvas: &Canvas,
        width: f32,
        height: f32,
        cache: &RenderCache,
        plan: &Plan,
    ) {
        if self.stack.is_empty() {
            return;
        }
        draw_dim_backdrop(canvas, width, height);
        if let Some(w) = self.stack.last() {
            w.render(canvas, width, height, cache, plan);
        }
    }

    pub fn on_cursor_moved(&mut self, x: f32, y: f32, w: f32, h: f32, plan: &Plan) -> DirtyRegion {
        let outcome = match self.stack.last_mut() {
            Some(win) => win.on_cursor_moved(x, y, w, h, plan),
            None => return DirtyRegion::None,
        };
        self.apply(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        w: f32,
        h: f32,
        modifiers: &Modifiers,
        plan: &Plan,
        sender: &PlanRequestSender,
        cache: &RenderCache,
    ) -> DirtyRegion {
        let outcome = match self.stack.last_mut() {
            Some(win) => win.on_mouse_input(x, y, pressed, w, h, modifiers, plan, sender, cache),
            None => return DirtyRegion::None,
        };
        if outcome.close {
            self.stack.pop();
            return DirtyRegion::All;
        }
        // Window is staying open — check for push requests.
        if let Some(new_win) = self.stack.last_mut().and_then(|w| w.take_open_request()) {
            self.push(new_win);
            DirtyRegion::All
        } else {
            outcome.dirty
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn on_key_input(
        &mut self,
        key: &Key,
        sender: &PlanRequestSender,
        width: f32,
        height: f32,
        plan: &Plan,
        cache: &RenderCache,
    ) -> DirtyRegion {
        let outcome = match self.stack.last_mut() {
            Some(w) => w.on_key_input(key, sender, width, height, plan, cache),
            None => return DirtyRegion::None,
        };
        if outcome.close {
            self.stack.pop();
            return DirtyRegion::All;
        }
        if let Some(new_win) = self.stack.last_mut().and_then(|w| w.take_open_request()) {
            self.push(new_win);
            DirtyRegion::All
        } else {
            outcome.dirty
        }
    }

    pub fn on_scroll(&mut self, delta_y: f32, plan: &Plan, width: f32, height: f32) -> DirtyRegion {
        let outcome = match self.stack.last_mut() {
            Some(w) => w.on_scroll(delta_y, plan, width, height),
            None => return DirtyRegion::None,
        };
        self.apply(outcome)
    }

    fn apply(&mut self, outcome: FloatingWindowOutcome) -> DirtyRegion {
        if outcome.close {
            self.stack.pop();
            DirtyRegion::All
        } else {
            outcome.dirty
        }
    }
}

/// Returns the actual panel dimensions given the window size and the window's
/// preferred maximum dimensions.  Each axis is `min(window * 0.95, max)`.
pub fn panel_size(window_w: f32, window_h: f32, max_w: f32, max_h: f32) -> (f32, f32) {
    ((window_w * 0.95).min(max_w), (window_h * 0.95).min(max_h))
}

fn draw_dim_backdrop(canvas: &Canvas, width: f32, height: f32) {
    let mut paint = Paint::default();
    paint.set_color(Color::from(OVERLAY_DARK));
    canvas.draw_rect(Rect::from_wh(width, height), &paint);
}
