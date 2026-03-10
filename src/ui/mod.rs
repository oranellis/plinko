//! Shared UI primitives used across all pages.
//!
//! - [`layout`]      — layout constants and colour palette.
//! - [`dirty`]       — [`DirtyRegion`](dirty::DirtyRegion) enum for partial-redraw tracking.
//! - [`cache`]       — [`RenderCache`](cache::RenderCache) of pre-built Skia paths / text blobs.
//! - [`icons`]       — Skia path builders for the three navigation icons.
//! - [`back_button`] — drawing and hit-testing for the back-navigation button.

pub mod back_button;
pub mod cache;
pub mod dirty;
pub mod icon_button;
pub mod icons;
pub mod layout;
pub mod text_input;
