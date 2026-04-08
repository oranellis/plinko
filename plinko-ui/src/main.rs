#![allow(dead_code)]
#![allow(unused_imports)]

mod app;
mod engine;
mod graphics;
mod monday;
mod pages;
mod ui;

use winit::event_loop::EventLoop;

fn parse_port_arg() -> Option<u16> {
    let args: Vec<String> = std::env::args().collect();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "-p" {
            return iter.next()?.parse().ok();
        }
        if let Some(val) = arg.strip_prefix("-p") {
            return val.parse().ok();
        }
    }
    None
}

fn main() {
    let el = EventLoop::new().expect("Failed to create event loop");
    let init = graphics::setup::initialize(&el);

    let port: u16 = parse_port_arg().unwrap_or_else(|| {
        std::env::var("PLINKO_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(7891)
    });

    let engine = match engine::NetworkEngine::connect(port) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to connect to plinko server: {e}");
            eprintln!("Make sure the plinko server is running on port {port}");
            std::process::exit(1);
        }
    };

    let mut application = app::Application::new(
        init.env,
        init.fb_info,
        init.num_samples,
        init.stencil_size,
        init.scale_factor,
        engine,
    );

    el.run_app(&mut application).expect("run() failed");
}
