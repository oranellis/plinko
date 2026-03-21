use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

use plinko_shared::data::Storage;
use plinko_shared::protocol::*;

use crate::engine::PlanEngine;

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
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    let hello = ServerMessage::Hello {
        version: VERSION.to_string(),
    };
    if send_msg(&mut writer, &hello).is_err() {
        return;
    }

    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let client_hello: ClientMessage = match serde_json::from_str(line.trim()) {
        Ok(m) => m,
        Err(_) => return,
    };
    let ClientMessage::Hello {
        version: client_version,
    } = client_hello
    else {
        return;
    };

    if client_version != VERSION {
        let _ = send_msg(
            &mut writer,
            &ServerMessage::VersionError {
                expected: VERSION.to_string(),
                got: client_version,
            },
        );
        return;
    }

    if send_msg(
        &mut writer,
        &ServerMessage::PlanState {
            plan: Box::new(engine.plan().clone()),
        },
    )
    .is_err()
    {
        return;
    }

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let msg: ClientMessage = match serde_json::from_str(line.trim()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("parse error: {e}");
                continue;
            }
        };
        let ClientMessage::Request { id, request } = msg else {
            continue;
        };

        if matches!(&request, PlanRequest::SavePlan) {
            if let Err(e) = storage.save(engine.plan()) {
                eprintln!("save error: {e}");
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
                    eprintln!("load error: {e}");
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
}
