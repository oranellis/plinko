use std::sync::{Arc, Mutex};

use plinko_shared::data::{Plan, Storage};
use plinko_shared::protocol::*;

use crate::auth::{AuthDb, SessionInfo};
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

/// Core protocol handler shared by WebSocket connections.
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
    auth_db: Arc<AuthDb>,
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

    // ── Auth phase ─────────────────────────────────────────────────────────
    // Send AuthRequired, then wait for Login or Authenticate message.
    if send(&ServerMessage::AuthRequired).is_err() {
        return;
    }

    let (session_token, session): (String, SessionInfo) = loop {
        let mut line = String::new();
        // WouldBlock means "no data yet" (WS uses a 50 ms recv timeout) — loop.
        loop {
            match recv(&mut line) {
                Ok(true) => break,
                Ok(false) => return,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    line.clear();
                    continue;
                }
                Err(_) => return,
            }
        }
        let msg: ClientMessage = match serde_json::from_str(line.trim()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[auth] {peer}: parse error: {e}");
                continue;
            }
        };
        match msg {
            ClientMessage::Login { email, password } => {
                match auth_db.login(&email, &password) {
                    Ok((token, info)) => {
                        let _ = send(&ServerMessage::LoginSuccess {
                            session_token: token.clone(),
                            user_id: info.user_id.clone(),
                            email: info.email.clone(),
                            is_admin: info.is_admin,
                        });
                        eprintln!("[auth] {peer}: login OK as {}", info.email);
                        break (token, info);
                    }
                    Err(e) => {
                        eprintln!("[auth] {peer}: login failed: {e}");
                        let _ = send(&ServerMessage::LoginFailed {
                            message: e.to_string(),
                        });
                        // Keep waiting for another attempt.
                    }
                }
            }
            ClientMessage::Authenticate { session_token } => {
                match auth_db.authenticate_token(&session_token) {
                    Ok(info) => {
                        let _ = send(&ServerMessage::LoginSuccess {
                            session_token: session_token.clone(),
                            user_id: info.user_id.clone(),
                            email: info.email.clone(),
                            is_admin: info.is_admin,
                        });
                        eprintln!("[auth] {peer}: token auth OK as {}", info.email);
                        break (session_token, info);
                    }
                    Err(e) => {
                        eprintln!("[auth] {peer}: token auth failed: {e}");
                        // Send AuthRequired again so the client shows the login form.
                        let _ = send(&ServerMessage::LoginFailed {
                            message: e.to_string(),
                        });
                        let _ = send(&ServerMessage::AuthRequired);
                    }
                }
            }
            _ => {
                // Ignore non-auth messages while in auth phase.
            }
        }
    };
    // ── End auth phase ─────────────────────────────────────────────────────

    let (initial_plan, initial_plan_id) = {
        let eng = engine.lock().unwrap();
        eprintln!(
            "[connect] {peer}: handshake OK (version {VERSION}), sending plan \"{}\"",
            eng.plan().name
        );
        let p = eng.plan().clone();
        let id = p.id;
        (p, id)
    };
    {
        let has_monday_integration = storage
            .lock()
            .unwrap()
            .load_monday_config(initial_plan_id)
            .is_some();
        if send(&ServerMessage::PlanState {
            plan: Box::new(initial_plan),
            has_monday_integration,
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
                    let incoming_plan_id = plan.id;
                    {
                        let mut eng = engine.lock().unwrap();
                        *eng = PlanEngine::new(*plan);
                        let _ = storage.lock().unwrap().save(eng.plan());
                    }
                    let _ = send(&ServerMessage::MondayDone { message });
                    let done_plan = engine.lock().unwrap().plan().clone();
                    let has_monday_integration = storage
                        .lock()
                        .unwrap()
                        .load_monday_config(incoming_plan_id)
                        .is_some();
                    let _ = send(&ServerMessage::PlanState {
                        plan: Box::new(done_plan),
                        has_monday_integration,
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
        // Handle Logout and auth requests before the Request dispatch.
        match &msg {
            ClientMessage::Logout => {
                let _ = auth_db.logout(&session_token);
                eprintln!("[{peer}] logout ({})", session.email);
                break;
            }
            _ => {}
        }
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
            let new_plan_clone = {
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
                eng.plan().clone()
            };
            let has_monday_integration = storage
                .lock()
                .unwrap()
                .load_monday_config(new_plan_clone.id)
                .is_some();
            if send(&ServerMessage::PlanState {
                plan: Box::new(new_plan_clone),
                has_monday_integration,
            })
            .is_err()
            {
                break;
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
                    let loaded_plan_clone = {
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
                        eng.plan().clone()
                    };
                    let has_monday_integration = storage
                        .lock()
                        .unwrap()
                        .load_monday_config(loaded_plan_clone.id)
                        .is_some();
                    if send(&ServerMessage::PlanState {
                        plan: Box::new(loaded_plan_clone),
                        has_monday_integration,
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
                let _ = tx.send(InternalMsg::Forward(ServerMessage::MondayProgress {
                    done: 0,
                    total: 0,
                    message: "Preparing push…".to_string(),
                }));
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

        // ── Auth PlanRequest handlers ───────────────────────────────────────
        if matches!(&request, PlanRequest::GetAuthUsers) {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            let users = auth_db.list_users().unwrap_or_default();
            // Map auth::AuthUser to protocol::AuthUser
            let proto_users: Vec<AuthUser> = users
                .into_iter()
                .map(|u| AuthUser {
                    id: u.id,
                    email: u.email,
                    is_admin: u.is_admin,
                })
                .collect();
            let _ = send(&ServerMessage::Response {
                id,
                response: PlanResponse::AuthUsers(proto_users),
            });
            continue;
        }

        if let PlanRequest::CreateAuthUser {
            email,
            password,
            is_admin,
        } = &request
        {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            match auth_db.create_user(email, password, *is_admin) {
                Ok(user_id) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::AuthUserCreated { user_id },
                    });
                }
                Err(e) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::Error(PlanError::AuthError(e.to_string())),
                    });
                }
            }
            continue;
        }

        if let PlanRequest::UpdateAuthUser {
            user_id,
            new_email,
            new_is_admin,
        } = &request
        {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            match auth_db.update_user(user_id, new_email.as_deref(), *new_is_admin) {
                Ok(()) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::PlanUpdated,
                    });
                }
                Err(e) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::Error(PlanError::AuthError(e.to_string())),
                    });
                }
            }
            continue;
        }

        if let PlanRequest::SetAuthUserPassword {
            user_id,
            new_password,
        } = &request
        {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            match auth_db.set_password(user_id, new_password) {
                Ok(()) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::PasswordChanged,
                    });
                }
                Err(e) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::Error(PlanError::AuthError(e.to_string())),
                    });
                }
            }
            continue;
        }

        if let PlanRequest::DeleteAuthUser { user_id } = &request {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            // Prevent self-deletion.
            if *user_id == session.user_id {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::AuthError(
                        "Cannot delete your own account".to_string(),
                    )),
                });
                continue;
            }
            match auth_db.delete_user(user_id) {
                Ok(()) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::PlanUpdated,
                    });
                }
                Err(e) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::Error(PlanError::AuthError(e.to_string())),
                    });
                }
            }
            continue;
        }

        if let PlanRequest::ChangeMyPassword {
            old_password,
            new_password,
        } = &request
        {
            match auth_db.change_own_password(&session.user_id, old_password, new_password) {
                Ok(()) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::PasswordChanged,
                    });
                }
                Err(e) => {
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::Error(PlanError::AuthError(e.to_string())),
                    });
                }
            }
            continue;
        }

        if let PlanRequest::GetUserLinks { plan_id } = &request {
            let links = storage.lock().unwrap().load_user_links(*plan_id);
            let _ = send(&ServerMessage::Response {
                id,
                response: PlanResponse::UserLinks(links),
            });
            continue;
        }

        if let PlanRequest::SetUserLinks { plan_id, links } = &request {
            storage.lock().unwrap().save_user_links(*plan_id, links);
            let _ = send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            });
            continue;
        }
        // ── End auth PlanRequest handlers ───────────────────────────────────

        let response = {
            let mut eng = engine.lock().unwrap();
            eng.apply_request(request)
        };
        let plan_changed = matches!(response, PlanResponse::PlanUpdated);
        if send(&ServerMessage::Response { id, response }).is_err() {
            break;
        }
        if plan_changed {
            let changed_plan = {
                let eng = engine.lock().unwrap();
                let _ = storage.lock().unwrap().save(eng.plan());
                eng.plan().clone()
            };
            let has_monday_integration = storage
                .lock()
                .unwrap()
                .load_monday_config(changed_plan.id)
                .is_some();
            if send(&ServerMessage::PlanState {
                plan: Box::new(changed_plan),
                has_monday_integration,
            })
            .is_err()
            {
                break;
            }
        }
    }
    eprintln!("[disconnect] {peer}: disconnected (served {request_count} requests)");
}
