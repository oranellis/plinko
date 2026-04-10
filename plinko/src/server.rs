use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use plinko_shared::data::{Plan, Storage};
use plinko_shared::protocol::*;

use crate::engine::PlanEngine;
use crate::monday::client::MondayClient;
use crate::monday::{export, import};

/// Internal messages sent from background Monday threads back to the connection loop.
enum InternalMsg {
    /// A ServerMessage to forward directly to the client.
    Forward(ServerMessage),
    /// A successfully imported plan — apply to engine and broadcast PlanState.
    ImportDone { plan: Box<Plan>, message: String },
}

pub fn run_server(engine: Arc<Mutex<PlanEngine>>, storage: Arc<Mutex<Storage>>, port: u16) {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind failed");
    eprintln!("plinko server listening on 127.0.0.1:{port}");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => handle_tcp_connection(s, Arc::clone(&engine), Arc::clone(&storage)),
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

pub(crate) fn send_line(writer: &mut impl Write, msg: &ServerMessage) -> std::io::Result<()> {
    let mut line = serde_json::to_string(msg).unwrap();
    line.push('\n');
    writer.write_all(line.as_bytes())
}

fn handle_tcp_connection(
    stream: TcpStream,
    engine: Arc<Mutex<PlanEngine>>,
    storage: Arc<Mutex<Storage>>,
) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    handle_protocol(
        peer,
        move |msg| send_line(&mut writer, msg),
        move |line| {
            line.clear();
            reader.read_line(line).map(|n| n > 0)
        },
        engine,
        storage,
    );
}

/// Core protocol handler — shared by TCP and WebSocket connections.
///
/// `send`: writes a `ServerMessage` to the client.
/// `recv`: reads the next message text into the provided `String`; returns
///         `Ok(true)` if data was received, `Ok(false)` on clean EOF, `Err` on error.
pub(crate) fn handle_protocol(
    peer: String,
    mut send: impl FnMut(&ServerMessage) -> std::io::Result<()>,
    mut recv: impl FnMut(&mut String) -> std::io::Result<bool>,
    engine: Arc<Mutex<PlanEngine>>,
    storage: Arc<Mutex<Storage>>,
) {
    eprintln!("[connect] client connected from {peer}");

    if send(&ServerMessage::Hello {
        version: VERSION.to_string(),
    })
    .is_err()
    {
        eprintln!("[connect] {peer}: failed to send Hello");
        return;
    }

    let mut line = String::new();
    match recv(&mut line) {
        Ok(false) | Err(_) => {
            eprintln!("[connect] {peer}: disconnected before handshake");
            return;
        }
        Ok(true) => {}
    }
    let client_hello: ClientMessage = match serde_json::from_str(line.trim()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[connect] {peer}: invalid handshake message: {e}");
            return;
        }
    };
    let ClientMessage::Hello {
        version: client_version,
    } = client_hello
    else {
        eprintln!("[connect] {peer}: expected Hello, got unexpected message type");
        return;
    };

    if client_version != VERSION {
        eprintln!("[connect] {peer}: version mismatch — server {VERSION}, client {client_version}");
        let _ = send(&ServerMessage::VersionError {
            expected: VERSION.to_string(),
            got: client_version,
        });
        return;
    }

    {
        let eng = engine.lock().unwrap();
        eprintln!(
            "[connect] {peer}: handshake OK (version {VERSION}), sending plan \"{}\"",
            eng.plan().name
        );
        if send(&ServerMessage::PlanState {
            plan: Box::new(eng.plan().clone()),
        })
        .is_err()
        {
            eprintln!("[connect] {peer}: failed to send initial PlanState");
            return;
        }
    }

    // Channel for Monday background threads to communicate with this connection loop.
    let (monday_tx, monday_rx) = std::sync::mpsc::channel::<InternalMsg>();

    let mut request_count: u64 = 0;
    loop {
        // Drain any pending Monday progress/done/error messages.
        while let Ok(msg) = monday_rx.try_recv() {
            match msg {
                InternalMsg::Forward(server_msg) => {
                    if send(&server_msg).is_err() {
                        return;
                    }
                }
                InternalMsg::ImportDone { plan, message } => {
                    let mut eng = engine.lock().unwrap();
                    *eng = PlanEngine::new(*plan);
                    let _ = storage.lock().unwrap().save(eng.plan());
                    let _ = send(&ServerMessage::MondayDone { message });
                    let _ = send(&ServerMessage::PlanState {
                        plan: Box::new(eng.plan().clone()),
                    });
                }
            }
        }

        let mut line = String::new();
        match recv(&mut line) {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Ok(false) | Err(_) => break,
            Ok(true) => {}
        }
        let msg: ClientMessage = match serde_json::from_str(line.trim()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[{peer}] parse error: {e}");
                continue;
            }
        };
        let ClientMessage::Request { id, request } = msg else {
            continue;
        };
        request_count += 1;

        if matches!(&request, PlanRequest::SavePlan) {
            eprintln!("[{peer}] SavePlan");
            let eng = engine.lock().unwrap();
            if let Err(e) = storage.lock().unwrap().save(eng.plan()) {
                eprintln!("[{peer}] save error: {e}");
            }
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        if matches!(&request, PlanRequest::NewPlan) {
            let new_plan = plinko_shared::data::Plan::new("New Plan");
            {
                let mut eng = engine.lock().unwrap();
                *eng = PlanEngine::new(new_plan);
                let _ = storage.lock().unwrap().save(eng.plan());
                if send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::PlanUpdated,
                })
                .is_err()
                {
                    break;
                }
                if send(&ServerMessage::PlanState {
                    plan: Box::new(eng.plan().clone()),
                })
                .is_err()
                {
                    break;
                }
            }
            continue;
        }

        if let PlanRequest::LoadPlan { plan_id } = &request {
            let plan_id = *plan_id;
            // Bind the result before matching so the MutexGuard is dropped immediately
            // (match scrutinee temporaries live for the entire match block; holding the
            // storage lock across the match would self-deadlock when line 223 tries to
            // re-acquire it).
            let load_result = storage.lock().unwrap().load_latest(plan_id);
            match load_result {
                Ok(plan) => {
                    let mut eng = engine.lock().unwrap();
                    *eng = PlanEngine::new(plan);
                    let _ = storage.lock().unwrap().save(eng.plan());
                    if send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::PlanUpdated,
                    })
                    .is_err()
                    {
                        break;
                    }
                    if send(&ServerMessage::PlanState {
                        plan: Box::new(eng.plan().clone()),
                    })
                    .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("[{peer}] load error: {e}");
                    if send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::PlanUpdated,
                    })
                    .is_err()
                    {
                        break;
                    }
                }
            }
            continue;
        }

        if matches!(&request, PlanRequest::ListPlans) {
            let plan_ids = storage.lock().unwrap().list_plans().unwrap_or_default();
            let mut list = Vec::new();
            for pid in plan_ids {
                if let Some((name, last_saved)) = storage.lock().unwrap().plan_summary(pid) {
                    list.push((pid, name, last_saved));
                }
            }
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanList(list),
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        if let PlanRequest::DeletePlan { plan_id } = &request {
            let plan_id = *plan_id;
            let _ = storage.lock().unwrap().delete_plan(plan_id);
            let plan_ids = storage.lock().unwrap().list_plans().unwrap_or_default();
            let mut list = Vec::new();
            for pid in plan_ids {
                if let Some((name, last_saved)) = storage.lock().unwrap().plan_summary(pid) {
                    list.push((pid, name, last_saved));
                }
            }
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanList(list),
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        if let PlanRequest::SetCurrentUser(uid) = &request {
            let uid = *uid;
            storage.lock().unwrap().save_current_user_id(uid);
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        // ── Monday.com requests ──────────────────────────────────────────────

        if let PlanRequest::LoadMondayConfig { plan_id } = &request {
            let plan_id = *plan_id;
            let stor = storage.lock().unwrap();
            let config = stor.load_monday_config(plan_id).unwrap_or_default();
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::MondayConfigLoaded(Box::new(config)),
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        if let PlanRequest::LoadMondayApiToken = &request {
            let token = storage.lock().unwrap().load_monday_api_token();
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::MondayApiToken(token),
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        if let PlanRequest::SaveMondayConfig {
            plan_id,
            config,
            token,
        } = request
        {
            let stor = storage.lock().unwrap();
            stor.save_monday_config(plan_id, &config);
            stor.save_monday_api_token(token.trim());
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        // MondayTestConnection runs synchronously so the response is sent directly.
        if let PlanRequest::MondayTestConnection { token, board_id: _ } = request {
            let client = MondayClient::new(&token);
            let response = match client.test_connection() {
                Ok(name) => PlanResponse::MondayConnected(format!("Connected as: {name}")),
                Err(e) => PlanResponse::Error(PlanError::Monday(e.to_string())),
            };
            if send(&ServerMessage::Response { id, response }).is_err() {
                break;
            }
            continue;
        }

        // MondayFetchBoardInfo runs synchronously — spawning a background thread caused a
        // deadlock because the server's recv() would block before the thread could put its
        // response into the channel to be forwarded.
        if let PlanRequest::MondayFetchBoardInfo { token, board_id } = request {
            let client = MondayClient::new(&token);
            let users = client.fetch_users().unwrap_or_default();
            let mut columns = client.fetch_columns(&board_id).unwrap_or_default();

            // Status columns typically live on the subitems board, not the parent board.
            // Fetch subitems board ID and include its columns so the user can map them.
            let subitem_board_id = client.fetch_subitem_board_id(&board_id).unwrap_or_default();
            let mut status_labels = vec![];
            if !subitem_board_id.is_empty() {
                let sub_columns = client.fetch_columns(&subitem_board_id).unwrap_or_default();
                // Find the first status-type column on the subitems board to populate labels.
                if let Some(status_col) = sub_columns
                    .iter()
                    .find(|c| c.column_type == "status" && c.title.to_lowercase() == "status")
                    .or_else(|| sub_columns.iter().find(|c| c.column_type == "status"))
                {
                    status_labels = client
                        .fetch_status_labels(&subitem_board_id, &status_col.id.clone())
                        .unwrap_or_default();
                }
                // Merge subitem columns into the column list (tagged so UI can distinguish them).
                for mut col in sub_columns {
                    col.title = format!("Subitems: {}", col.title);
                    columns.push(col);
                }
            }

            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::MondayBoardInfo {
                    users,
                    columns,
                    status_labels,
                },
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        if let PlanRequest::MondayPull { plan_id } | PlanRequest::MondayFullReimport { plan_id } =
            &request
        {
            let plan_id = *plan_id;
            let is_reimport = matches!(request, PlanRequest::MondayFullReimport { .. });
            let config = storage
                .lock()
                .unwrap()
                .load_monday_config(plan_id)
                .unwrap_or_default();
            let token = storage.lock().unwrap().load_monday_api_token();
            let mut plan_clone = engine.lock().unwrap().plan().clone();
            // For FullReimport: strip all previously-imported tasks and clear the
            // item_node_map so import_from_monday treats every Monday item as new.
            // Without clearing, import would find all items in `existing`, enter the
            // (silent-fail) update path instead of the create path, and produce a plan
            // with no Monday tasks while the old IDs remain in the map — causing each
            // subsequent reimport to add another full set of duplicate tasks.
            let mut import_config = config.clone();
            if is_reimport {
                // Clear everything so we start completely fresh — the whole
                // point of Full Re-import is a clean slate, not a partial diff.
                plan_clone.tasks.clear();
                plan_clone.milestones.clear();
                import_config.item_node_map.clear();
            }
            let tx = monday_tx.clone();
            let storage_clone = Arc::clone(&storage);
            std::thread::spawn(move || {
                let client = MondayClient::new(&token);
                let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayProgress {
                    done: 0,
                    total: 0,
                    message: "Fetching data from Monday...".to_string(),
                }));
                match import::import_from_monday(&client, &import_config, plan_clone) {
                    Ok((new_plan, new_map, message)) => {
                        let mut updated_config = import_config.clone();
                        updated_config.item_node_map = new_map;
                        storage_clone
                            .lock()
                            .unwrap()
                            .save_monday_config(plan_id, &updated_config);
                        let _ = tx.send(InternalMsg::ImportDone {
                            plan: Box::new(new_plan),
                            message,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayError {
                            message: e.to_string(),
                        }));
                    }
                }
            });
            // Send immediate ack so the client's sendRequest promise resolves.
            // The actual result arrives as MondayDone/MondayError push messages.
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        if let PlanRequest::MondayPush { plan_id } = &request {
            let plan_id = *plan_id;
            let config = storage
                .lock()
                .unwrap()
                .load_monday_config(plan_id)
                .unwrap_or_default();
            let token = storage.lock().unwrap().load_monday_api_token();
            let plan_snapshot = engine.lock().unwrap().plan().clone();
            let item_node_map = config.item_node_map.clone();
            let tx = monday_tx.clone();
            let storage_clone = Arc::clone(&storage);
            std::thread::spawn(move || {
                let client = MondayClient::new(&token);
                let result = export::export_to_monday_diff(
                    &client,
                    &config,
                    &plan_snapshot,
                    &item_node_map,
                    |done, total, msg| {
                        let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayProgress {
                            done,
                            total,
                            message: msg.to_string(),
                        }));
                    },
                );
                match result {
                    Ok((message, new_map)) => {
                        let mut updated_config = config.clone();
                        updated_config.item_node_map = new_map;
                        storage_clone
                            .lock()
                            .unwrap()
                            .save_monday_config(plan_id, &updated_config);
                        let _ =
                            tx.send(InternalMsg::Forward(ServerMessage::MondayDone { message }));
                    }
                    Err(e) => {
                        let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayError {
                            message: e.to_string(),
                        }));
                    }
                }
            });
            if send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            })
            .is_err()
            {
                break;
            }
            continue;
        }

        let response = {
            let mut eng = engine.lock().unwrap();
            eng.apply_request(request)
        };
        let plan_changed = matches!(response, PlanResponse::PlanUpdated);
        if send(&ServerMessage::Response { id, response }).is_err() {
            break;
        }
        if plan_changed {
            let eng = engine.lock().unwrap();
            let _ = storage.lock().unwrap().save(eng.plan());
            if send(&ServerMessage::PlanState {
                plan: Box::new(eng.plan().clone()),
            })
            .is_err()
            {
                break;
            }
        }
    }
    eprintln!("[disconnect] {peer}: disconnected (served {request_count} requests)");
}
