use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, Modifiers, MouseButton, WindowEvent},
    event_loop::ControlFlow,
};

use glutin::prelude::GlSurface;
use skia_safe::{
    gpu::{self, gl::FramebufferInfo},
    ClipOp, Color, ImageInfo, Picture, PictureRecorder, Rect, Surface,
};

use crate::graphics::env::{self, Env};
use crate::pages::{PageId, PageManager};
use crate::pages::home::render as home_render;
use crate::ui::back_button;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::layout::{BACK_BTN_X, BACK_BTN_Y, BACK_BTN_SIZE, HOME_BG, PANEL_BG};

#[derive(Clone, Copy, PartialEq)]
enum AppState {
    Home,
    InPage(PageId),
}

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
    // GPU-backed retained surface for partial redraws
    retained_surface: Option<Surface>,
    retained_size: (i32, i32),
    // Cached pictures
    home_picture: Option<Picture>,
    back_picture: Option<Picture>,
}

impl Application {
    pub fn new(
        env: Env,
        fb_info: FramebufferInfo,
        num_samples: usize,
        stencil_size: usize,
        scale_factor: f64,
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
            retained_surface: None,
            retained_size: (0, 0),
            home_picture: None,
            back_picture: None,
        }
    }

    fn logical_size(&self) -> (f32, f32) {
        let phys = self.env.window.inner_size();
        let sf = self.scale_factor as f32;
        (phys.width as f32 / sf, phys.height as f32 / sf)
    }

    fn to_logical(&self, px: f64, py: f64) -> (f32, f32) {
        let sf = self.scale_factor as f32;
        (px as f32 / sf, py as f32 / sf)
    }

    fn mark_dirty(&mut self, region: DirtyRegion) {
        self.pending_dirty = self.pending_dirty.merge(region);
    }

    fn navigate_to(&mut self, page: PageId) {
        self.app_state = AppState::InPage(page);
        self.pages.set_active(page);
        self.home_picture = None;
        self.back_picture = None;
        self.mark_dirty(DirtyRegion::All);
    }

    fn navigate_home(&mut self) {
        self.app_state = AppState::Home;
        self.pages.set_active(PageId::Home);
        self.home_picture = None;
        self.back_picture = None;
        self.mark_dirty(DirtyRegion::All);
    }
}

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
                event: KeyEvent { logical_key, .. },
                ..
            } => {
                if self.modifiers.state().super_key() && logical_key == "q" {
                    event_loop.exit();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = self.to_logical(position.x, position.y);
                self.cursor_pos = (x, y);

                match self.app_state {
                    AppState::Home => {
                        let dirty = self.pages.active_page_mut().on_cursor_moved(x, y, width, height);
                        if dirty != DirtyRegion::None {
                            self.home_picture = None;
                        }
                        self.mark_dirty(dirty);
                    }
                    AppState::InPage(_) => {
                        // Back button hover
                        let new_back_hovered = back_button::hit_test_back_button(x, y);
                        if new_back_hovered != self.back_hovered {
                            self.back_hovered = new_back_hovered;
                            self.back_picture = None;
                            self.mark_dirty(DirtyRegion::BackButtonOnly);
                        }

                        // Forward to active page
                        let dirty = self.pages.active_page_mut().on_cursor_moved(x, y, width, height);
                        self.mark_dirty(dirty);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    let (x, y) = self.cursor_pos;

                    match state {
                        ElementState::Pressed => {
                            match self.app_state {
                                AppState::Home => {
                                    if let Some(idx) = home_render::hit_test_card(x, y, width, height) {
                                        match idx {
                                            0 => self.navigate_to(PageId::Daily),
                                            1 => self.navigate_to(PageId::Planning),
                                            2 => self.navigate_to(PageId::Settings),
                                            _ => {}
                                        }
                                    }
                                }
                                AppState::InPage(_) => {
                                    if back_button::hit_test_back_button(x, y) {
                                        self.navigate_home();
                                    } else {
                                        let dirty = self.pages.active_page_mut().on_mouse_input(
                                            x, y, true, width, height,
                                        );
                                        self.mark_dirty(dirty);
                                    }
                                }
                            }
                        }
                        ElementState::Released => {
                            if let AppState::InPage(_) = self.app_state {
                                let dirty = self.pages.active_page_mut().on_mouse_input(
                                    x, y, false, width, height,
                                );
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
                    self.retained_surface = create_retained_surface(
                        &mut self.env.gr_context,
                        phys_size.0,
                        phys_size.1,
                    );
                    self.retained_size = phys_size;
                    self.home_picture = None;
                    self.back_picture = None;
                    self.pending_dirty = DirtyRegion::All;
                }

                let dirty = self.pending_dirty;
                self.pending_dirty = DirtyRegion::None;

                if dirty != DirtyRegion::None
                    && let Some(retained) = &mut self.retained_surface {
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

                                match dirty {
                                    DirtyRegion::All => {
                                        canvas.clear(Color::from(PANEL_BG));
                                        self.pages.active_page().render(canvas, width, height, &self.cache);
                                        if let Some(pic) = &self.back_picture {
                                            canvas.draw_picture(pic, None, None);
                                        }
                                    }
                                    DirtyRegion::PageOnly => {
                                        canvas.clear(Color::from(PANEL_BG));
                                        self.pages.active_page().render(canvas, width, height, &self.cache);
                                        if let Some(pic) = &self.back_picture {
                                            canvas.draw_picture(pic, None, None);
                                        }
                                    }
                                    DirtyRegion::BackButtonOnly => {
                                        canvas.save();
                                        canvas.clip_rect(
                                            Rect::from_xywh(BACK_BTN_X, BACK_BTN_Y, BACK_BTN_SIZE, BACK_BTN_SIZE),
                                            ClipOp::Intersect,
                                            false,
                                        );
                                        self.pages.active_page().render(canvas, width, height, &self.cache);
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
            _ => (),
        }

        if self.pending_dirty != DirtyRegion::None {
            self.env.window.request_redraw();
        }

        event_loop.set_control_flow(ControlFlow::Wait);
    }
}
