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

    // Try to load from storage; fall back to a blank plan.
    let storage = match data::Storage::from_user_data_dir() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storage init error: {e}, using in-memory only");
            data::Storage::from_path("/tmp/plinko-fallback")
        }
    };

    // Resolve previously-saved current user (before we have a plan).
    let saved_user_id = storage.load_current_user_id();

    // Load the most recently saved plan, or create a new one.
    let plan = {
        let ids = storage.list_plans().unwrap_or_default();
        // Pick the most recently modified plan: the one whose latest snapshot is newest.
        let mut best: Option<(String, uuid::Uuid)> = None;
        for id in ids {
            if let Ok(versions) = storage.list_versions(id)
                && let Some(latest) = versions.last()
                && best.as_ref().is_none_or(|(b, _)| latest > b)
            {
                best = Some((latest.clone(), id));
            }
        }
        if let Some((_, id)) = best {
            storage.load_latest(id).unwrap_or_else(|e| {
                eprintln!("load error: {e}, creating new plan");
                data::Plan::new("My Plan")
            })
        } else {
            data::Plan::new("My Plan")
        }
    };

    // Resolve current_user to a UserId that exists in the loaded plan.
    let current_user =
        saved_user_id.and_then(|uid| plan.users_data.contains_key(&uid).then_some(uid));

    let engine = engine::PlanEngine::new(plan);

    let mut application = app::Application::new(
        init.env,
        init.fb_info,
        init.num_samples,
        init.stencil_size,
        init.scale_factor,
        engine,
        storage,
        current_user,
    );

    el.run_app(&mut application).expect("run() failed");
}
