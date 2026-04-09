use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

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

pub fn run_server(mut engine: PlanEngine, mut storage: Storage, port: u16) {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind failed");
    eprintln!("plinko server listening on 127.0.0.1:{port}");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => handle_connection(s, &mut engine, &mut storage),
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

fn send_msg(writer: &mut impl Write, msg: &ServerMessage) -> std::io::Result<()> {
    let mut line = serde_json::to_string(msg).unwrap();
    line.push('\n');
    writer.write_all(line.as_bytes())
}

fn handle_connection(stream: TcpStream, engine: &mut PlanEngine, storage: &mut Storage) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    eprintln!("[connect] client connected from {peer}");

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    let hello = ServerMessage::Hello {
        version: VERSION.to_string(),
    };
    if send_msg(&mut writer, &hello).is_err() {
        eprintln!("[connect] {peer}: failed to send Hello");
        return;
    }

    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        eprintln!("[connect] {peer}: disconnected before handshake");
        return;
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
        let _ = send_msg(
            &mut writer,
            &ServerMessage::VersionError {
                expected: VERSION.to_string(),
                got: client_version,
            },
        );
        return;
    }

    eprintln!(
        "[connect] {peer}: handshake OK (version {VERSION}), sending plan \"{}\"",
        engine.plan().name
    );

    if send_msg(
        &mut writer,
        &ServerMessage::PlanState {
            plan: Box::new(engine.plan().clone()),
        },
    )
    .is_err()
    {
        eprintln!("[connect] {peer}: failed to send initial PlanState");
        return;
    }

    // Channel for Monday background threads to send messages back to this connection loop.
    let (monday_tx, monday_rx) = std::sync::mpsc::channel::<InternalMsg>();

    let mut request_count: u64 = 0;
    loop {
        // Drain any pending Monday progress/done/error messages first.
        while let Ok(msg) = monday_rx.try_recv() {
            match msg {
                InternalMsg::Forward(server_msg) => {
                    if send_msg(&mut writer, &server_msg).is_err() {
                        return;
                    }
                }
                InternalMsg::ImportDone { plan, message } => {
                    *engine = PlanEngine::new(*plan);
                    let _ = storage.save(engine.plan());
                    let _ = send_msg(&mut writer, &ServerMessage::MondayDone { message });
                    let _ = send_msg(
                        &mut writer,
                        &ServerMessage::PlanState {
                            plan: Box::new(engine.plan().clone()),
                        },
                    );
                }
            }
        }

        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
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
            if let Err(e) = storage.save(engine.plan()) {
                eprintln!("[{peer}] save error: {e}");
            }
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            continue;
        }

        if matches!(&request, PlanRequest::NewPlan) {
            let new_plan = plinko_shared::data::Plan::new("New Plan");
            *engine = PlanEngine::new(new_plan);
            let _ = storage.save(engine.plan());
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            if send_msg(
                &mut writer,
                &ServerMessage::PlanState {
                    plan: Box::new(engine.plan().clone()),
                },
            )
            .is_err()
            {
                break;
            }
            continue;
        }

        if let PlanRequest::LoadPlan { plan_id } = &request {
            let plan_id = *plan_id;
            match storage.load_latest(plan_id) {
                Ok(plan) => {
                    *engine = PlanEngine::new(plan);
                    let resp = ServerMessage::Response {
                        id,
                        response: PlanResponse::PlanUpdated,
                    };
                    if send_msg(&mut writer, &resp).is_err() {
                        break;
                    }
                    if send_msg(
                        &mut writer,
                        &ServerMessage::PlanState {
                            plan: Box::new(engine.plan().clone()),
                        },
                    )
                    .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("[{peer}] load error: {e}");
                    let resp = ServerMessage::Response {
                        id,
                        response: PlanResponse::PlanUpdated,
                    };
                    if send_msg(&mut writer, &resp).is_err() {
                        break;
                    }
                }
            }
            continue;
        }

        if matches!(&request, PlanRequest::ListPlans) {
            let plan_ids = storage.list_plans().unwrap_or_default();
            let mut list = Vec::new();
            for pid in plan_ids {
                if let Some((name, last_saved)) = storage.plan_summary(pid) {
                    list.push((pid, name, last_saved));
                }
            }
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanList(list),
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            continue;
        }

        if let PlanRequest::DeletePlan { plan_id } = &request {
            let plan_id = *plan_id;
            let _ = storage.delete_plan(plan_id);
            // Return a refreshed plan list so the UI updates immediately.
            let plan_ids = storage.list_plans().unwrap_or_default();
            let mut list = Vec::new();
            for pid in plan_ids {
                if let Some((name, last_saved)) = storage.plan_summary(pid) {
                    list.push((pid, name, last_saved));
                }
            }
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanList(list),
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            continue;
        }

        if let PlanRequest::SetCurrentUser(uid) = &request {
            let uid = *uid;
            storage.save_current_user_id(uid);
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            continue;
        }

        // ── Monday.com requests ───────────────────────────────────────────────

        if let PlanRequest::LoadMondayConfig { plan_id } = &request {
            let plan_id = *plan_id;
            let config = storage.load_monday_config(plan_id).unwrap_or_default();
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::MondayConfigLoaded(Box::new(config)),
            };
            if send_msg(&mut writer, &resp).is_err() {
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
            storage.save_monday_config(plan_id, &config);
            storage.save_monday_api_token(token.trim());
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            continue;
        }

        if let PlanRequest::MondayTestConnection { token, board_id } = request {
            let tx = monday_tx.clone();
            std::thread::spawn(move || {
                let client = MondayClient::new(&token);
                match client.test_connection() {
                    Ok(name) => {
                        let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayDone {
                            message: format!("Connected as: {name}"),
                        }));
                    }
                    Err(e) => {
                        let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayError {
                            message: e.to_string(),
                        }));
                    }
                }
                drop(board_id);
            });
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            continue;
        }

        if let PlanRequest::MondayFetchBoardInfo { token, board_id } = request {
            let tx = monday_tx.clone();
            std::thread::spawn(move || {
                let client = MondayClient::new(&token);
                let users = client.fetch_users().unwrap_or_default();
                let columns = client.fetch_columns(&board_id).unwrap_or_default();
                let status_labels = client
                    .fetch_status_labels(&board_id, "")
                    .unwrap_or_default();
                let _ = tx.send(InternalMsg::Forward(ServerMessage::Response {
                    id,
                    response: PlanResponse::MondayBoardInfo {
                        users,
                        columns,
                        status_labels,
                    },
                }));
            });
            // Acknowledge receipt; actual result arrives via InternalMsg::Forward.
            continue;
        }

        if let PlanRequest::MondayPull { plan_id } | PlanRequest::MondayFullReimport { plan_id } =
            &request
        {
            let plan_id = *plan_id;
            let is_reimport = matches!(request, PlanRequest::MondayFullReimport { .. });
            let config = storage.load_monday_config(plan_id).unwrap_or_default();
            let token = storage.load_monday_api_token();
            let mut plan_clone = engine.plan().clone();
            if is_reimport {
                let mapped_node_ids: std::collections::HashSet<_> = config
                    .item_node_map
                    .iter()
                    .map(|m| m.plinko_node_id)
                    .collect();
                plan_clone.tasks.retain(|id, _| {
                    !mapped_node_ids.contains(&plinko_shared::data::ids::NodeId::Task(*id))
                });
                plan_clone.milestones.retain(|id, _| {
                    !mapped_node_ids.contains(&plinko_shared::data::ids::NodeId::Milestone(*id))
                });
            }
            let tx = monday_tx.clone();
            let storage_clone = storage.clone();
            std::thread::spawn(move || {
                let client = MondayClient::new(&token);
                let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayProgress {
                    done: 0,
                    total: 0,
                    message: "Fetching data from Monday...".to_string(),
                }));
                match import::import_from_monday(&client, &config, plan_clone) {
                    Ok((new_plan, new_map, message)) => {
                        let mut updated_config = config.clone();
                        updated_config.item_node_map = new_map;
                        storage_clone.save_monday_config(plan_id, &updated_config);
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
            continue;
        }

        if let PlanRequest::MondayPush { plan_id } = &request {
            let plan_id = *plan_id;
            let config = storage.load_monday_config(plan_id).unwrap_or_default();
            let token = storage.load_monday_api_token();
            let plan_snapshot = engine.plan().clone();
            let item_node_map = config.item_node_map.clone();
            let tx = monday_tx.clone();
            let storage_clone = storage.clone();
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
                        storage_clone.save_monday_config(plan_id, &updated_config);
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
            let resp = ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            };
            if send_msg(&mut writer, &resp).is_err() {
                break;
            }
            continue;
        }

        let response = engine.apply_request(request);
        let plan_changed = matches!(response, PlanResponse::PlanUpdated);
        let resp_msg = ServerMessage::Response { id, response };
        if send_msg(&mut writer, &resp_msg).is_err() {
            break;
        }
        if plan_changed {
            let _ = storage.save(engine.plan());
            if send_msg(
                &mut writer,
                &ServerMessage::PlanState {
                    plan: Box::new(engine.plan().clone()),
                },
            )
            .is_err()
            {
                break;
            }
        }
    }
    eprintln!("[disconnect] {peer}: disconnected (served {request_count} requests)");
}
