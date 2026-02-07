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
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::layout::{BUTTON_COUNT, PANEL_BG, TOOLBAR_HEIGHT};
use crate::ui::toolbar;

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
    hovered_button: Option<usize>,
    // GPU-backed retained surface for partial redraws
    retained_surface: Option<Surface>,
    retained_size: (i32, i32),
    // Cached toolbar Picture (display list)
    toolbar_picture: Option<Picture>,
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
            hovered_button: None,
            retained_surface: None,
            retained_size: (0, 0),
            toolbar_picture: None,
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

    fn handle_button_click(&mut self, idx: usize) {
        match idx {
            0 => {
                self.pages.set_active(PageId::Daily);
                self.toolbar_picture = None;
                self.mark_dirty(DirtyRegion::All);
            }
            1 => {
                self.pages.set_active(PageId::Planning);
                self.toolbar_picture = None;
                self.mark_dirty(DirtyRegion::All);
            }
            2 => {
                self.pages.set_active(PageId::Settings);
                self.toolbar_picture = None;
                self.mark_dirty(DirtyRegion::All);
            }
            3 => println!("Undo"),
            4 => println!("Redo"),
            _ => {}
        }
    }
}

fn record_toolbar_picture(
    width: f32,
    active_page: PageId,
    hovered_button: Option<usize>,
    icon_paths: &[skia_safe::Path; BUTTON_COUNT],
) -> Picture {
    let bounds = Rect::from_wh(width, TOOLBAR_HEIGHT);
    let mut recorder = PictureRecorder::new();
    {
        let canvas = recorder.begin_recording(bounds, false);
        toolbar::draw_toolbar(canvas, width, active_page, hovered_button, icon_paths);
    }
    recorder
        .finish_recording_as_picture(None)
        .expect("Failed to record toolbar picture")
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
        let page_top = TOOLBAR_HEIGHT;
        let page_height = height - page_top;

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                self.toolbar_picture = None;
                self.mark_dirty(DirtyRegion::All);
            }
            WindowEvent::Resized(_physical_size) => {
                env::resize_surface(
                    &mut self.env,
                    self.fb_info,
                    self.num_samples,
                    self.stencil_size,
                );
                self.toolbar_picture = None;
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

                // Toolbar hover
                let new_hovered = toolbar::hit_test_button(x, y);
                if new_hovered != self.hovered_button {
                    self.hovered_button = new_hovered;
                    self.toolbar_picture = None;
                    self.mark_dirty(DirtyRegion::ToolbarOnly);
                }

                // Page cursor events (only below toolbar)
                if y > page_top {
                    let page_y = y - page_top;
                    let dirty = self
                        .pages
                        .active_page_mut()
                        .on_cursor_moved(x, page_y, width, page_height);
                    self.mark_dirty(dirty);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    let (x, y) = self.cursor_pos;

                    match state {
                        ElementState::Pressed => {
                            if let Some(idx) = toolbar::hit_test_button(x, y) {
                                self.handle_button_click(idx);
                            } else if y > page_top {
                                let page_y = y - page_top;
                                let dirty = self.pages.active_page_mut().on_mouse_input(
                                    x,
                                    page_y,
                                    true,
                                    width,
                                    page_height,
                                );
                                self.mark_dirty(dirty);
                            }
                        }
                        ElementState::Released => {
                            if y > page_top {
                                let page_y = y - page_top;
                                let dirty = self.pages.active_page_mut().on_mouse_input(
                                    x,
                                    page_y,
                                    false,
                                    width,
                                    page_height,
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
                    self.toolbar_picture = None;
                    self.pending_dirty = DirtyRegion::All;
                }

                let dirty = self.pending_dirty;
                self.pending_dirty = DirtyRegion::None;

                // Ensure toolbar picture is current
                if self.toolbar_picture.is_none() {
                    self.toolbar_picture = Some(record_toolbar_picture(
                        width,
                        self.pages.active,
                        self.hovered_button,
                        &self.cache.icon_paths,
                    ));
                }

                // Re-render only dirty regions to retained surface
                if dirty != DirtyRegion::None {
                    if let Some(retained) = &mut self.retained_surface {
                        let canvas = retained.canvas();
                        canvas.save();
                        canvas.scale((sf, sf));

                        match dirty {
                            DirtyRegion::All => {
                                canvas.clear(Color::from(PANEL_BG));
                                if let Some(pic) = &self.toolbar_picture {
                                    canvas.draw_picture(pic, None, None);
                                }
                                canvas.save();
                                canvas.translate((0.0, page_top));
                                self.pages
                                    .active_page()
                                    .render(canvas, width, page_height, &self.cache);
                                canvas.restore();
                            }
                            DirtyRegion::ToolbarOnly => {
                                canvas.save();
                                canvas.clip_rect(
                                    Rect::from_xywh(0.0, 0.0, width, TOOLBAR_HEIGHT),
                                    ClipOp::Intersect,
                                    false,
                                );
                                if let Some(pic) = &self.toolbar_picture {
                                    canvas.draw_picture(pic, None, None);
                                }
                                canvas.restore();
                            }
                            DirtyRegion::PageOnly => {
                                canvas.save();
                                canvas.clip_rect(
                                    Rect::from_xywh(0.0, page_top, width, page_height),
                                    ClipOp::Intersect,
                                    false,
                                );
                                canvas.translate((0.0, page_top));
                                self.pages
                                    .active_page()
                                    .render(canvas, width, page_height, &self.cache);
                                canvas.restore();
                            }
                            DirtyRegion::None => {}
                        }

                        canvas.restore();
                    }
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
