//! Top-level application state and winit event handling.
//!
//! [`Application`] owns all runtime state — the OpenGL/Skia environment, the
//! render cache, all page instances, dirty-region tracking, and the retained
//! off-screen surface used for partial redraws.  It implements winit's
//! [`ApplicationHandler`] to translate window events into render and navigation
//! decisions.

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ControlFlow,
};

use glutin::prelude::GlSurface;
use skia_safe::{
    BlendMode, ClipOp, Color, ImageInfo, Picture, PictureRecorder, Rect, Surface,
    gpu::{self, gl::FramebufferInfo},
};

use crate::engine::{PlanEngine, PlanResponse};
use crate::graphics::env::{self, Env};
use crate::pages::home::render as home_render;
use crate::pages::{Page, PageId, PageManager};
use crate::ui::back_button;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::FloatingWindowManager;
use crate::ui::layout::{BACK_BTN_SIZE, BACK_BTN_X, BACK_BTN_Y, HOME_BG, PANEL_BG};

/// Tracks whether the user is on the home screen or inside a specific page.
#[derive(Clone, Copy, PartialEq)]
enum AppState {
    /// The home card-grid is visible.
    Home,
    /// A full-screen page is open.
    InPage(PageId),
}

/// Root application struct implementing the winit [`ApplicationHandler`] trait.
///
/// Owns every piece of mutable runtime state:
/// - The OpenGL/Skia [`Env`] and framebuffer parameters.
/// - A [`RenderCache`] of pre-built Skia paths and text blobs.
/// - A [`PageManager`] holding all page instances.
/// - [`DirtyRegion`] accumulator for the current event cycle.
/// - A GPU-backed off-screen `retained_surface` for partial redraws.
/// - Skia `Picture` caches for the home view and back button (invalidated on
///   hover/navigation/resize so only changed regions are re-recorded).
pub struct Application {
    pub env: Env,
    pub fb_info: FramebufferInfo,
    pub num_samples: usize,
    pub stencil_size: usize,
    modifiers: Modifiers,
    scale_factor: f64,
    cache: RenderCache,
    pending_dirty: DirtyRegion,
    pages: PageManager,
    cursor_pos: (f32, f32),
    app_state: AppState,
    back_hovered: bool,
    /// Plan engine: owns the live Plan and the request queue.
    engine: PlanEngine,
    /// GPU-backed retained surface for partial redraws.
    retained_surface: Option<Surface>,
    retained_size: (i32, i32),
    /// Cached Skia Picture for the home card grid (invalidated on hover/resize).
    home_picture: Option<Picture>,
    /// Cached Skia Picture for the back button (invalidated on hover/navigate/resize).
    back_picture: Option<Picture>,
    /// Stack of floating windows (modal overlays).
    floats: FloatingWindowManager,
}

impl Application {
    pub fn new(
        env: Env,
        fb_info: FramebufferInfo,
        num_samples: usize,
        stencil_size: usize,
        scale_factor: f64,
        engine: PlanEngine,
    ) -> Self {
        Self {
            env,
            fb_info,
            num_samples,
            stencil_size,
            modifiers: Modifiers::default(),
            scale_factor,
            cache: RenderCache::new(),
            pending_dirty: DirtyRegion::All,
            pages: PageManager::new(),
            cursor_pos: (0.0, 0.0),
            app_state: AppState::Home,
            back_hovered: false,
            engine,
            retained_surface: None,
            retained_size: (0, 0),
            home_picture: None,
            back_picture: None,
            floats: FloatingWindowManager::new(),
        }
    }

    /// Returns the window size in logical (DPI-independent) pixels.
    fn logical_size(&self) -> (f32, f32) {
        let phys = self.env.window.inner_size();
        let sf = self.scale_factor as f32;
        (phys.width as f32 / sf, phys.height as f32 / sf)
    }

    /// Converts physical pixel coordinates to logical coordinates.
    fn to_logical(&self, px: f64, py: f64) -> (f32, f32) {
        let sf = self.scale_factor as f32;
        (px as f32 / sf, py as f32 / sf)
    }

    /// Merges `region` into the pending dirty accumulator so all dirty
    /// regions within an event cycle are coalesced before the next redraw.
    fn mark_dirty(&mut self, region: DirtyRegion) {
        self.pending_dirty = self.pending_dirty.merge(region);
    }

    /// Switches to a full-screen page and invalidates all caches.
    fn navigate_to(&mut self, page: PageId) {
        // Clear hover on the page we're leaving (home) and the one we're entering.
        self.pages.home.reset_hover();
        self.app_state = AppState::InPage(page);
        self.pages.set_active(page);
        self.pages.active_page_mut().reset_hover();
        self.back_hovered = false;
        self.home_picture = None;
        self.back_picture = None;
        self.mark_dirty(DirtyRegion::All);
    }

    /// Returns to the home card-grid and invalidates all caches.
    fn navigate_home(&mut self) {
        // Clear hover on the page we're leaving and on home.
        self.pages.active_page_mut().reset_hover();
        self.app_state = AppState::Home;
        self.pages.set_active(PageId::Home);
        self.pages.home.reset_hover();
        self.back_hovered = false;
        self.home_picture = None;
        self.back_picture = None;
        self.floats = FloatingWindowManager::new();
        self.mark_dirty(DirtyRegion::All);
    }
}

/// Allocates a GPU-backed off-screen Skia surface at the given physical pixel
/// dimensions.  Used as the retained intermediate buffer for partial redraws.
fn create_retained_surface(
    gr_context: &mut gpu::DirectContext,
    width: i32,
    height: i32,
) -> Option<Surface> {
    let image_info = ImageInfo::new_n32_premul((width.max(1), height.max(1)), None);
    gpu::surfaces::render_target(
        gr_context,
        gpu::Budgeted::Yes,
        &image_info,
        None,
        None,
        None,
        None,
        None,
    )
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let (width, height) = self.logical_size();

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                self.home_picture = None;
                self.back_picture = None;
                self.mark_dirty(DirtyRegion::All);
            }
            WindowEvent::Resized(_physical_size) => {
                env::resize_surface(
                    &mut self.env,
                    self.fb_info,
                    self.num_samples,
                    self.stencil_size,
                );
                self.home_picture = None;
                self.back_picture = None;
                self.mark_dirty(DirtyRegion::All);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => self.modifiers = new_modifiers,
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: key_state,
                        ..
                    },
                ..
            } => {
                if self.modifiers.state().super_key() && logical_key == "q" {
                    event_loop.exit();
                }
                if key_state == ElementState::Pressed
                    && let AppState::InPage(_) = self.app_state
                {
                    let sender = self.engine.sender();
                    let dirty = if self.floats.is_open() {
                        self.floats.on_key_input(&logical_key, &sender)
                    } else {
                        self.pages
                            .active_page_mut()
                            .on_key_input(&logical_key, &sender)
                    };
                    self.mark_dirty(dirty);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = self.to_logical(position.x, position.y);
                self.cursor_pos = (x, y);

                match self.app_state {
                    AppState::Home => {
                        let plan = self.engine.plan();
                        let dirty = self
                            .pages
                            .active_page_mut()
                            .on_cursor_moved(x, y, width, height, plan);
                        if dirty != DirtyRegion::None {
                            self.home_picture = None;
                        }
                        self.mark_dirty(dirty);
                    }
                    AppState::InPage(_) => {
                        // Back button is topmost — check before forwarding to page.
                        let prev_back_hovered = self.back_hovered;
                        let new_back_hovered = back_button::hit_test_back_button(x, y);
                        if new_back_hovered != prev_back_hovered {
                            self.back_hovered = new_back_hovered;
                            self.back_picture = None;
                            self.mark_dirty(DirtyRegion::BackButtonOnly);
                        }

                        if new_back_hovered {
                            // Back button is covering this position; don't let the page
                            // show hover effects for content beneath it.
                            // On the transition in, send (-1, -1) to clear any stale
                            // page hover state left from before the cursor entered.
                            if !prev_back_hovered {
                                let plan = self.engine.plan();
                                let dirty = self
                                    .pages
                                    .active_page_mut()
                                    .on_cursor_moved(-1.0, -1.0, width, height, plan);
                                self.mark_dirty(dirty);
                            }
                        } else {
                            let plan = self.engine.plan();
                            let dirty = if self.floats.is_open() {
                                self.floats.on_cursor_moved(x, y, width, height, plan)
                            } else {
                                self.pages
                                    .active_page_mut()
                                    .on_cursor_moved(x, y, width, height, plan)
                            };
                            self.mark_dirty(dirty);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    let (x, y) = self.cursor_pos;

                    match state {
                        ElementState::Pressed => match self.app_state {
                            AppState::Home => {
                                if let Some(idx) = home_render::hit_test_card(x, y, width, height) {
                                    match idx {
                                        0 => self.navigate_to(PageId::Daily),
                                        1 => self.navigate_to(PageId::Overview),
                                        2 => self.navigate_to(PageId::Settings),
                                        _ => {}
                                    }
                                }
                            }
                            AppState::InPage(_) => {
                                if back_button::hit_test_back_button(x, y) {
                                    self.navigate_home();
                                } else if self.floats.is_open() {
                                    let plan = self.engine.plan();
                                    let sender = self.engine.sender();
                                    let dirty = self.floats.on_mouse_input(
                                        x,
                                        y,
                                        true,
                                        width,
                                        height,
                                        plan,
                                        &sender,
                                        &self.cache,
                                    );
                                    self.mark_dirty(dirty);
                                } else {
                                    let plan = self.engine.plan();
                                    let sender = self.engine.sender();
                                    let dirty = self
                                        .pages
                                        .active_page_mut()
                                        .on_mouse_input(x, y, true, width, height, plan, &sender);
                                    self.mark_dirty(dirty);
                                    if let Some(window) =
                                        self.pages.active_page_mut().take_open_request()
                                    {
                                        self.pages.active_page_mut().reset_hover();
                                        self.floats.push(window);
                                        self.mark_dirty(DirtyRegion::All);
                                    }
                                }
                            }
                        },
                        ElementState::Released => {
                            if let AppState::InPage(_) = self.app_state {
                                let plan = self.engine.plan();
                                let sender = self.engine.sender();
                                let dirty = if self.floats.is_open() {
                                    self.floats.on_mouse_input(
                                        x,
                                        y,
                                        false,
                                        width,
                                        height,
                                        plan,
                                        &sender,
                                        &self.cache,
                                    )
                                } else {
                                    self.pages
                                        .active_page_mut()
                                        .on_mouse_input(x, y, false, width, height, plan, &sender)
                                };
                                self.mark_dirty(dirty);
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let sf = self.scale_factor as f32;
                let phys = self.env.window.inner_size();
                let phys_size = (phys.width as i32, phys.height as i32);

                // Ensure retained surface exists at correct size
                if self.retained_size != phys_size {
                    self.retained_surface =
                        create_retained_surface(&mut self.env.gr_context, phys_size.0, phys_size.1);
                    self.retained_size = phys_size;
                    self.home_picture = None;
                    self.back_picture = None;
                    self.pending_dirty = DirtyRegion::All;
                }

                let dirty = self.pending_dirty;
                self.pending_dirty = DirtyRegion::None;

                if dirty != DirtyRegion::None
                    && let Some(retained) = &mut self.retained_surface
                {
                    let canvas = retained.canvas();
                    canvas.save();
                    canvas.scale((sf, sf));

                    match self.app_state {
                        AppState::Home => {
                            // Rebuild home picture if invalidated
                            if self.home_picture.is_none() {
                                let bounds = Rect::from_wh(width, height);
                                let mut recorder = PictureRecorder::new();
                                {
                                    let rec_canvas = recorder.begin_recording(bounds, false);
                                    home_render::draw_home(
                                        rec_canvas,
                                        width,
                                        height,
                                        self.pages.home.state.hovered_card,
                                        &self.cache,
                                    );
                                }
                                self.home_picture = recorder.finish_recording_as_picture(None);
                            }

                            canvas.clear(Color::from(HOME_BG));
                            if let Some(pic) = &self.home_picture {
                                canvas.draw_picture(pic, None, None);
                            }
                        }
                        AppState::InPage(_) => {
                            // Rebuild back button picture if invalidated
                            if self.back_picture.is_none() {
                                let bounds = Rect::from_wh(
                                    BACK_BTN_X + BACK_BTN_SIZE + 1.0,
                                    BACK_BTN_Y + BACK_BTN_SIZE + 1.0,
                                );
                                let mut recorder = PictureRecorder::new();
                                {
                                    let rec_canvas = recorder.begin_recording(bounds, false);
                                    back_button::draw_back_button(rec_canvas, self.back_hovered);
                                }
                                self.back_picture = recorder.finish_recording_as_picture(None);
                            }

                            let plan = self.engine.plan();
                            match dirty {
                                DirtyRegion::All => {
                                    canvas.clear(Color::from(PANEL_BG));
                                    self.pages.active_page().render(
                                        canvas,
                                        width,
                                        height,
                                        &self.cache,
                                        plan,
                                    );
                                    self.floats.render(canvas, width, height, &self.cache, plan);
                                    if let Some(pic) = &self.back_picture {
                                        canvas.draw_picture(pic, None, None);
                                    }
                                }
                                DirtyRegion::PageOnly => {
                                    canvas.clear(Color::from(PANEL_BG));
                                    self.pages.active_page().render(
                                        canvas,
                                        width,
                                        height,
                                        &self.cache,
                                        plan,
                                    );
                                    self.floats.render(canvas, width, height, &self.cache, plan);
                                    if let Some(pic) = &self.back_picture {
                                        canvas.draw_picture(pic, None, None);
                                    }
                                }
                                DirtyRegion::BackButtonOnly => {
                                    canvas.save();
                                    canvas.clip_rect(
                                        Rect::from_xywh(
                                            BACK_BTN_X,
                                            BACK_BTN_Y,
                                            BACK_BTN_SIZE,
                                            BACK_BTN_SIZE,
                                        ),
                                        ClipOp::Intersect,
                                        false,
                                    );
                                    // Clear the clipped region to the page background before
                                    // re-drawing — pages that don't fill this area (e.g. the
                                    // blank overview page) would otherwise leave the previous
                                    // hover state visible on the retained surface.
                                    canvas.draw_color(Color::from(PANEL_BG), BlendMode::Src);
                                    self.pages.active_page().render(
                                        canvas,
                                        width,
                                        height,
                                        &self.cache,
                                        plan,
                                    );
                                    if let Some(pic) = &self.back_picture {
                                        canvas.draw_picture(pic, None, None);
                                    }
                                    canvas.restore();
                                }
                                DirtyRegion::None => {}
                            }
                        }
                    }

                    canvas.restore();
                }

                // Composite retained surface to framebuffer (cheap GPU blit)
                if let Some(retained) = &mut self.retained_surface {
                    let image = retained.image_snapshot();
                    let fb_canvas = self.env.surface.canvas();
                    fb_canvas.draw_image(image, (0, 0), None);
                }

                self.env.gr_context.flush(None);
                self.env.window.pre_present_notify();
                self.env
                    .gl_surface
                    .swap_buffers(&self.env.gl_context)
                    .unwrap();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let AppState::InPage(_) = self.app_state
                    && self.floats.is_open()
                {
                    let delta_y = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
                    };
                    let (width, height) = self.logical_size();
                    let plan = self.engine.plan();
                    let dirty = self.floats.on_scroll(delta_y, plan, width, height);
                    self.mark_dirty(dirty);
                }
            }
            _ => (),
        }

        // Drain the plan request queue and handle responses.
        for response in self.engine.process_pending() {
            match response {
                PlanResponse::PlanUpdated => self.mark_dirty(DirtyRegion::All),
                PlanResponse::Error(e) => {
                    // TODO: surface errors to the UI
                    eprintln!("plan error: {e}");
                }
            }
        }

        if self.pending_dirty != DirtyRegion::None {
            self.env.window.request_redraw();
        }

        event_loop.set_control_flow(ControlFlow::Wait);
    }
}
