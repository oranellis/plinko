//! OpenGL + Skia initialisation and environment.
//!
//! - [`setup`] — one-shot bootstrap that creates the window, GL context, and
//!   Skia [`DirectContext`](skia_safe::gpu::DirectContext).
//! - [`env`] — the [`Env`](env::Env) struct that keeps those handles alive,
//!   plus helpers for (re)creating the Skia surface on resize.

pub mod env;
pub mod setup;
