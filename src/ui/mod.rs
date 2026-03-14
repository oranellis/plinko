//! Shared UI primitives used across all pages.
//!
//! - [`layout`]      — layout constants and colour palette.
//! - [`dirty`]       — [`DirtyRegion`](dirty::DirtyRegion) enum for partial-redraw tracking.
//! - [`cache`]       — [`RenderCache`](cache::RenderCache) of pre-built Skia paths / text blobs.
//! - [`icons`]       — Skia path builders for the three navigation icons.
//! - [`back_button`] — drawing and hit-testing for the back-navigation button.

pub mod avatar;
pub mod back_button;
pub mod cache;
pub mod dirty;
pub mod floating_window;
pub mod icon_button;
pub mod icons;
pub mod layout;
pub mod milestone_form_window;
pub mod multi_line_input;
pub mod plan_settings_window;
pub mod schedule_window;
pub mod tags_window;
pub mod task_form_window;
pub mod text_input;
pub mod user_form_window;
pub mod users_window;
pub mod window_chrome;
