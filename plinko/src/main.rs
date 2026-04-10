mod auth;
mod engine;
mod monday;
mod server;
mod static_server;
mod ws_server;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use auth::AuthDb;
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

    let engine = Arc::new(Mutex::new(engine::PlanEngine::new(plan)));
    let storage = Arc::new(Mutex::new(storage));

    // Initialise the auth database.
    let auth_db_path = {
        let s = storage.lock().unwrap();
        AuthDb::default_path(s.plans_dir())
    };
    let auth_db = match AuthDb::open(&auth_db_path) {
        Ok(db) => {
            if let Err(e) = db.bootstrap_root() {
                eprintln!("auth bootstrap error: {e}");
            }
            Arc::new(db)
        }
        Err(e) => {
            eprintln!("auth db open error: {e} — auth disabled");
            // Fall back to an in-memory DB so the server still starts.
            let db = AuthDb::open(":memory:").expect("in-memory auth db failed");
            let _ = db.bootstrap_root();
            Arc::new(db)
        }
    };

    let ws_port = port + 1;
    let engine_ws = Arc::clone(&engine);
    let storage_ws = Arc::clone(&storage);
    let auth_ws = Arc::clone(&auth_db);
    std::thread::spawn(move || {
        ws_server::run_ws_server(engine_ws, storage_ws, auth_ws, ws_port);
    });

    // Serve the React app's built assets on port+2 if the dist directory exists.
    let static_port = port + 2;
    let dist_dir: PathBuf = std::env::var("PLINKO_WEB_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let exe = std::env::current_exe().unwrap_or_default();
            exe.parent()
                .unwrap_or(std::path::Path::new("."))
                .join("plinko-web/dist")
        });
    if dist_dir.exists() {
        std::thread::spawn(move || {
            static_server::run_static_server(dist_dir, static_port);
        });
    } else {
        eprintln!(
            "static server: dist dir not found at {}, skipping (run `npm run build` in plinko-web/)",
            dist_dir.display()
        );
    }

    server::run_server(engine, storage, auth_db, port);
}
