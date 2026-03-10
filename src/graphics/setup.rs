//! One-shot OpenGL + Skia bootstrap.
//!
//! Call [`initialize`] once at startup with the winit [`EventLoop`] to obtain
//! an [`InitResult`] that is passed directly into [`Application::new`](crate::app::Application::new).

use std::{ffi::CString, num::NonZeroU32};

use gl::types::*;
use gl_rs as gl;
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder},
    display::{GetGlDisplay, GlDisplay},
    prelude::NotCurrentGlContext,
    surface::{SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use skia_safe::gpu::gl::FramebufferInfo;
use winit::{dpi::LogicalSize, event_loop::EventLoop, window::WindowAttributes};

use super::env::{Env, create_surface};

/// Everything [`initialize`] produces.  Passed straight into
/// [`Application::new`](crate::app::Application::new).
pub struct InitResult {
    /// All live OpenGL/Skia handles.
    pub env: Env,
    /// Identifies the default framebuffer for Skia's GL backend.
    pub fb_info: FramebufferInfo,
    /// MSAA sample count chosen by glutin.
    pub num_samples: usize,
    /// Stencil buffer size chosen by glutin.
    pub stencil_size: usize,
    /// Window DPI scale factor (physical px / logical px).
    pub scale_factor: f64,
}

/// Bootstraps the entire OpenGL + Skia stack.
///
/// Steps performed:
/// 1. Creates a winit window via glutin-winit's [`DisplayBuilder`].
/// 2. Selects the best GL config (preferring transparency, fewest samples).
/// 3. Creates and makes current a GL context (falls back to GLES on failure).
/// 4. Loads all GL function pointers.
/// 5. Creates a Skia [`DirectContext`](skia_safe::gpu::DirectContext) and wraps
///    the default framebuffer in a Skia [`Surface`](skia_safe::Surface).
pub fn initialize(el: &EventLoop<()>) -> InitResult {
    let window_attributes = WindowAttributes::default()
        .with_title("Skia Toolbar + Split View")
        .with_inner_size(LogicalSize::new(800, 600));

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(true);

    let display_builder = DisplayBuilder::new().with_window_attributes(window_attributes.into());
    let (window, gl_config) = display_builder
        .build(el, template, |configs| {
            configs
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);

                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .unwrap();
    println!("Picked a config with {} samples", gl_config.num_samples());
    let window = window.expect("Could not create window with OpenGL context");
    let window_handle = window
        .window_handle()
        .expect("Failed to retrieve RawWindowHandle");
    let raw_window_handle = window_handle.as_raw();

    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));

    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));
    let not_current_gl_context = unsafe {
        gl_config
            .display()
            .create_context(&gl_config, &context_attributes)
            .unwrap_or_else(|_| {
                gl_config
                    .display()
                    .create_context(&gl_config, &fallback_context_attributes)
                    .expect("failed to create context")
            })
    };

    let (width, height): (u32, u32) = window.inner_size().into();

    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
    );

    let gl_surface = unsafe {
        gl_config
            .display()
            .create_window_surface(&gl_config, &attrs)
            .expect("Could not create gl window surface")
    };

    let gl_context = not_current_gl_context
        .make_current(&gl_surface)
        .expect("Could not make GL context current when setting up skia renderer");

    gl::load_with(|s| {
        gl_config
            .display()
            .get_proc_address(CString::new(s).unwrap().as_c_str())
    });
    let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
        if name == "eglGetCurrentDisplay" {
            return std::ptr::null();
        }
        gl_config
            .display()
            .get_proc_address(CString::new(name).unwrap().as_c_str())
    })
    .expect("Could not create interface");

    let mut gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
        .expect("Could not create direct context");

    let fb_info = {
        let mut fboid: GLint = 0;
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

        FramebufferInfo {
            fboid: fboid.try_into().unwrap(),
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        }
    };

    let num_samples = gl_config.num_samples() as usize;
    let stencil_size = gl_config.stencil_size() as usize;
    let scale_factor = window.scale_factor();

    let surface = create_surface(&window, fb_info, &mut gr_context, num_samples, stencil_size);

    let env = Env {
        surface,
        gl_surface,
        gl_context,
        gr_context,
        window,
    };

    InitResult {
        env,
        fb_info,
        num_samples,
        stencil_size,
        scale_factor,
    }
}
