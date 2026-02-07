use std::num::NonZeroU32;

use glutin::{
    context::PossiblyCurrentContext,
    prelude::GlSurface,
    surface::{Surface as GlutinSurface, WindowSurface},
};
use skia_safe::{
    ColorType, Surface,
    gpu::{self, SurfaceOrigin, backend_render_targets, gl::FramebufferInfo},
};
use winit::window::Window;

pub struct Env {
    pub surface: Surface,
    pub gl_surface: GlutinSurface<WindowSurface>,
    pub gr_context: skia_safe::gpu::DirectContext,
    pub gl_context: PossiblyCurrentContext,
    pub window: Window,
}

impl Drop for Env {
    fn drop(&mut self) {
        self.gr_context.release_resources_and_abandon();
    }
}

pub fn create_surface(
    window: &Window,
    fb_info: FramebufferInfo,
    gr_context: &mut skia_safe::gpu::DirectContext,
    num_samples: usize,
    stencil_size: usize,
) -> Surface {
    let size = window.inner_size();
    let size = (
        size.width.try_into().expect("Could not convert width"),
        size.height.try_into().expect("Could not convert height"),
    );
    let backend_render_target =
        backend_render_targets::make_gl(size, num_samples, stencil_size, fb_info);

    gpu::surfaces::wrap_backend_render_target(
        gr_context,
        &backend_render_target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
    .expect("Could not create skia surface")
}

pub fn resize_surface(env: &mut Env, fb_info: FramebufferInfo, num_samples: usize, stencil_size: usize) {
    env.surface = create_surface(&env.window, fb_info, &mut env.gr_context, num_samples, stencil_size);
    let size = env.window.inner_size();
    env.gl_surface.resize(
        &env.gl_context,
        NonZeroU32::new(size.width.max(1)).unwrap(),
        NonZeroU32::new(size.height.max(1)).unwrap(),
    );
}
