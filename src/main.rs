//! Application entry point.
//!
//! Initialises the OpenGL/Skia environment via [`graphics::setup`], constructs
//! the [`app::Application`] state machine, and hands control to the winit event
//! loop.

// Allow unused code throughout the codebase - the data layer is fully implemented
// but not all functionality is used by the UI layer yet.
#![allow(dead_code)]
#![allow(unused_imports)]

mod app;
mod data;
mod engine;
mod graphics;
mod pages;
mod ui;

use winit::event_loop::EventLoop;

/// Creates the winit event loop, initialises OpenGL + Skia, then runs the app.
fn main() {
    let el = EventLoop::new().expect("Failed to create event loop");

    let init = graphics::setup::initialize(&el);

    let plan = data::Plan::new("My Plan");
    let engine = engine::PlanEngine::new(plan);

    let mut application = app::Application::new(
        init.env,
        init.fb_info,
        init.num_samples,
        init.stencil_size,
        init.scale_factor,
        engine,
    );

    el.run_app(&mut application).expect("run() failed");
}
