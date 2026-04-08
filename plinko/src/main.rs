mod engine;
mod server;

use plinko_shared::data::{Plan, Storage};

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
    let storage = match Storage::from_user_data_dir() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storage init error: {e}, using in-memory only");
            Storage::from_path("/tmp/plinko-fallback")
        }
    };

    let plan = {
        let ids = storage.list_plans().unwrap_or_default();
        let mut best: Option<(String, uuid::Uuid)> = None;
        for id in ids {
            if let Ok(versions) = storage.list_versions(id)
                && let Some(latest) = versions.last()
                && best.as_ref().is_none_or(|(b, _)| latest > b)
            {
                best = Some((latest.clone(), id));
            }
        }
        if let Some((_, id)) = best {
            storage.load_latest(id).unwrap_or_else(|e| {
                eprintln!("load error: {e}, creating new plan");
                Plan::new("My Plan")
            })
        } else {
            Plan::new("My Plan")
        }
    };

    let plan = if !plan.node_allocations.has_schedule() {
        let mut engine = engine::PlanEngine::new(plan);
        let _ = engine.apply_request(plinko_shared::protocol::PlanRequest::RunScheduler);
        engine.into_plan()
    } else {
        plan
    };

    let port: u16 = parse_port_arg().unwrap_or_else(|| {
        std::env::var("PLINKO_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(7891)
    });

    let engine = engine::PlanEngine::new(plan);
    server::run_server(engine, storage, port);
}
