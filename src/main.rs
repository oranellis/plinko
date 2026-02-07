mod app;
mod graphics;
mod pages;
mod ui;

use winit::event_loop::EventLoop;

fn main() {
    let el = EventLoop::new().expect("Failed to create event loop");

    let init = graphics::setup::initialize(&el);

    let mut application = app::Application::new(
        init.env,
        init.fb_info,
        init.num_samples,
        init.stencil_size,
        init.scale_factor,
    );

    el.run_app(&mut application).expect("run() failed");
}
