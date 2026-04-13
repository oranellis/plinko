use std::sync::{Arc, Mutex};

use plinko_shared::data::{Plan, Storage};
use plinko_shared::protocol::*;

use crate::auth::{AuthDb, SessionInfo};
use crate::engine::PlanEngine;
use crate::monday::client::MondayClient;
use crate::monday::{export, import};
use crate::ws_server::SessionRegistry;

/// Internal messages sent from background Monday threads back to the connection loop.
enum InternalMsg {
    /// A ServerMessage to forward directly to the client.
    Forward(ServerMessage),
    /// A successfully imported plan — apply to engine and broadcast PlanState.
    ImportDone { plan: Box<Plan>, message: String },
}

/// Broadcast a `PlanState` message to all sessions in the registry *except* `my_id`.
///
/// Conflict policy: **last-writer-wins**. All plan mutations are serialised through
/// `Arc<Mutex<PlanEngine>>`, so concurrent edits are applied in arrival order.  The
/// full `PlanState` snapshot is sent after every mutation, ensuring every client
/// converges to the same state.  No optimistic locking or operational transformation
/// is applied — if two users edit the same field simultaneously, the last write wins.
fn broadcast_plan_state(my_id: u64, registry: &SessionRegistry, plan: &Plan, has_monday: bool) {
    let msg = ServerMessage::PlanState {
        plan: Box::new(plan.clone()),
        has_monday_integration: has_monday,
    };
    let reg = registry.lock().unwrap();
    for (&id, sender) in reg.iter() {
        if id != my_id {
            let _ = sender.send(msg.clone());
        }
    }
}

/// Core protocol handler shared by WebSocket connections.
///
/// `send`: writes a `ServerMessage` to the client.
/// `recv`: reads the next message text into the provided `String`; returns
///         `Ok(true)` if data was received, `Ok(false)` on clean EOF, `Err` on error.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_protocol(
    peer: String,
    mut send: impl FnMut(&ServerMessage) -> std::io::Result<()>,
    mut recv: impl FnMut(&mut String) -> std::io::Result<bool>,
    engine: Arc<Mutex<Option<PlanEngine>>>,
    storage: Arc<Mutex<Storage>>,
    auth_db: Arc<AuthDb>,
    session_id: u64,
    registry: SessionRegistry,
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
                        let prefs = UserPrefs {
                            last_plan_id: auth_db.get_user_last_plan(&info.user_id),
                        };
                        let _ = send(&ServerMessage::LoginSuccess {
                            session_token: token.clone(),
                            user_id: info.user_id.clone(),
                            email: info.email.clone(),
                            is_admin: info.is_admin,
                            user_prefs: prefs,
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
                        let prefs = UserPrefs {
                            last_plan_id: auth_db.get_user_last_plan(&info.user_id),
                        };
                        let _ = send(&ServerMessage::LoginSuccess {
                            session_token: session_token.clone(),
                            user_id: info.user_id.clone(),
                            email: info.email.clone(),
                            is_admin: info.is_admin,
                            user_prefs: prefs,
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

    // Determine the initial state to send. If no plan is active, try to auto-load
    // from the user's last-plan preference.
    //
    // All disk I/O is done *before* acquiring the engine lock so the lock is held
    // only for the brief check-and-set, preventing concurrent connections from
    // blocking each other for the duration of file reads.
    let auto_load_candidate: Option<Plan> =
        if engine.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
            if let Some(plan_id) = auth_db.get_user_last_plan(&session.user_id) {
                let all_ids = storage.lock().unwrap().list_plans().unwrap_or_default();
                let visible =
                    auth_db.filter_visible_plans(&session.user_id, session.is_admin, &all_ids);
                if visible.contains(&plan_id) {
                    eprintln!("[connect] {peer}: auto-loading plan {plan_id}");
                    match storage.lock().unwrap().load_latest(plan_id) {
                        Ok(plan) => Some(plan),
                        Err(e) => {
                            eprintln!("[connect] {peer}: auto-load failed: {e}");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

    let initial_plan_info: Option<(Plan, bool)> = {
        let mut eng_guard = engine.lock().unwrap_or_else(|e| e.into_inner());
        // If the engine is still empty and we pre-loaded a plan, set it now.
        if eng_guard.is_none()
            && let Some(plan) = auto_load_candidate
        {
            *eng_guard = Some(PlanEngine::new(plan));
        }
        if let Some(eng) = eng_guard.as_ref() {
            let plan = eng.plan().clone();
            let plan_id = plan.id;
            let has_monday = storage
                .lock()
                .unwrap()
                .load_monday_config(plan_id)
                .is_some();
            Some((plan, has_monday))
        } else {
            None
        }
    };

    match initial_plan_info {
        Some((plan, has_monday)) => {
            eprintln!(
                "[connect] {peer}: handshake OK (version {VERSION}), sending plan \"{}\"",
                plan.name
            );
            if send(&ServerMessage::PlanState {
                plan: Box::new(plan),
                has_monday_integration: has_monday,
            })
            .is_err()
            {
                eprintln!("[connect] {peer}: failed to send initial PlanState");
                return;
            }
        }
        None => {
            eprintln!("[connect] {peer}: handshake OK (version {VERSION}), no active plan");
            if send(&ServerMessage::NoPlanActive).is_err() {
                eprintln!("[connect] {peer}: failed to send NoPlanActive");
                return;
            }
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
                        *eng = Some(PlanEngine::new(*plan));
                        let _ = storage.lock().unwrap().save(eng.as_ref().unwrap().plan());
                    }
                    let _ = send(&ServerMessage::MondayDone { message });
                    let done_plan = engine.lock().unwrap().as_ref().map(|e| e.plan().clone());
                    if let Some(done_plan) = done_plan {
                        let has_monday_integration = storage
                            .lock()
                            .unwrap()
                            .load_monday_config(incoming_plan_id)
                            .is_some();
                        let _ = send(&ServerMessage::PlanState {
                            plan: Box::new(done_plan.clone()),
                            has_monday_integration,
                        });
                        broadcast_plan_state(
                            session_id,
                            &registry,
                            &done_plan,
                            has_monday_integration,
                        );
                    }
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
        if let ClientMessage::Logout = &msg {
            let _ = auth_db.logout(&session_token);
            eprintln!("[{peer}] logout ({})", session.email);
            break;
        }
        let ClientMessage::Request { id, request } = msg else {
            continue;
        };
        request_count += 1;

        if matches!(&request, PlanRequest::SavePlan) {
            eprintln!("[{peer}] SavePlan");
            let eng = engine.lock().unwrap();
            if let Some(eng) = eng.as_ref()
                && let Err(e) = storage.lock().unwrap().save(eng.plan())
            {
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
                *eng = Some(PlanEngine::new(new_plan));
                let eng_ref = eng.as_ref().unwrap();
                let _ = storage.lock().unwrap().save(eng_ref.plan());
                if send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::PlanUpdated,
                })
                .is_err()
                {
                    break;
                }
                eng_ref.plan().clone()
            };
            let _ = auth_db.set_user_last_plan(&session.user_id, Some(new_plan_clone.id));
            let has_monday_integration = storage
                .lock()
                .unwrap()
                .load_monday_config(new_plan_clone.id)
                .is_some();
            if send(&ServerMessage::PlanState {
                plan: Box::new(new_plan_clone.clone()),
                has_monday_integration,
            })
            .is_err()
            {
                break;
            }
            broadcast_plan_state(
                session_id,
                &registry,
                &new_plan_clone,
                has_monday_integration,
            );
            continue;
        }

        if let PlanRequest::LoadPlan { plan_id } = &request {
            let plan_id = *plan_id;
            let load_result = storage.lock().unwrap().load_latest(plan_id);
            match load_result {
                Ok(plan) => {
                    let loaded_plan_clone = {
                        let mut eng = engine.lock().unwrap();
                        *eng = Some(PlanEngine::new(plan));
                        let eng_ref = eng.as_ref().unwrap();
                        let _ = storage.lock().unwrap().save(eng_ref.plan());
                        if send(&ServerMessage::Response {
                            id,
                            response: PlanResponse::PlanUpdated,
                        })
                        .is_err()
                        {
                            break;
                        }
                        eng_ref.plan().clone()
                    };
                    let _ =
                        auth_db.set_user_last_plan(&session.user_id, Some(loaded_plan_clone.id));
                    let has_monday_integration = storage
                        .lock()
                        .unwrap()
                        .load_monday_config(loaded_plan_clone.id)
                        .is_some();
                    if send(&ServerMessage::PlanState {
                        plan: Box::new(loaded_plan_clone.clone()),
                        has_monday_integration,
                    })
                    .is_err()
                    {
                        break;
                    }
                    broadcast_plan_state(
                        session_id,
                        &registry,
                        &loaded_plan_clone,
                        has_monday_integration,
                    );
                }
                Err(e) => {
                    eprintln!("[{peer}] load error: {e}");
                    if send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::Error(PlanError::AuthError(e.to_string())),
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
            let all_ids = storage.lock().unwrap().list_plans().unwrap_or_default();
            let visible_ids =
                auth_db.filter_visible_plans(&session.user_id, session.is_admin, &all_ids);
            let mut list = Vec::new();
            for pid in visible_ids {
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
            let _ = auth_db.set_plan_visibility(plan_id, &[]); // clean up visibility
            let _ = storage.lock().unwrap().delete_plan(plan_id);
            let all_ids = storage.lock().unwrap().list_plans().unwrap_or_default();
            let visible_ids =
                auth_db.filter_visible_plans(&session.user_id, session.is_admin, &all_ids);
            let mut list = Vec::new();
            for pid in visible_ids {
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

        if let PlanRequest::SetCurrentUser(_) = &request {
            // Current plan user preference is stored per auth user in user_prefs (auth.db).
            // This request is handled after user-prefs support is added; acknowledge it.
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
            let plan_clone_opt = {
                let eng_guard = engine.lock().unwrap();
                eng_guard.as_ref().map(|e| e.plan().clone())
            };
            let Some(mut plan_clone) = plan_clone_opt else {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::NoPlanActive),
                });
                continue;
            };
            let config = storage
                .lock()
                .unwrap()
                .load_monday_config(plan_id)
                .unwrap_or_default();
            let token = storage.lock().unwrap().load_monday_api_token();
            let mut import_config = config.clone();
            if is_reimport {
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

        if let PlanRequest::MondayPushPreview { plan_id } = &request {
            let plan_id = *plan_id;
            let plan_snapshot_opt = {
                let eng_guard = engine.lock().unwrap();
                eng_guard.as_ref().map(|e| e.plan().clone())
            };
            let Some(plan_snapshot) = plan_snapshot_opt else {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::NoPlanActive),
                });
                continue;
            };
            let config = storage
                .lock()
                .unwrap()
                .load_monday_config(plan_id)
                .unwrap_or_default();
            let token = storage.lock().unwrap().load_monday_api_token();
            let item_node_map = config.item_node_map.clone();
            let client = MondayClient::new(&token);
            let response =
                match export::preview_push_counts(&client, &config, &plan_snapshot, &item_node_map)
                {
                    Ok((op_count, new_item_count)) => PlanResponse::MondayPushPreview {
                        op_count,
                        new_item_count,
                    },
                    Err(e) => PlanResponse::Error(PlanError::Monday(e.to_string())),
                };
            if send(&ServerMessage::Response { id, response }).is_err() {
                break;
            }
            continue;
        }

        if let PlanRequest::MondayPush { plan_id } = &request {
            let plan_id = *plan_id;
            let plan_snapshot_opt = {
                let eng_guard = engine.lock().unwrap();
                eng_guard.as_ref().map(|e| e.plan().clone())
            };
            let Some(plan_snapshot) = plan_snapshot_opt else {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::NoPlanActive),
                });
                continue;
            };
            let config = storage
                .lock()
                .unwrap()
                .load_monday_config(plan_id)
                .unwrap_or_default();
            let token = storage.lock().unwrap().load_monday_api_token();
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

        if let PlanRequest::GetPlanVisibility { plan_id } = &request {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            let user_ids = auth_db.get_plan_visibility(*plan_id).unwrap_or_default();
            let _ = send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanVisibility {
                    plan_id: *plan_id,
                    user_ids,
                },
            });
            continue;
        }

        if let PlanRequest::SetPlanVisibility { plan_id, user_ids } = &request {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            let _ = auth_db.set_plan_visibility(*plan_id, user_ids);
            let _ = send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanUpdated,
            });
            continue;
        }

        // ── Version history ────────────────────────────────────────────────
        if let PlanRequest::ListPlanVersions { plan_id } = &request {
            let versions = storage
                .lock()
                .unwrap()
                .list_versions(*plan_id)
                .unwrap_or_default();
            let _ = send(&ServerMessage::Response {
                id,
                response: PlanResponse::PlanVersionList(versions),
            });
            continue;
        }

        if let PlanRequest::RestorePlanVersion { plan_id, version } = &request {
            if !session.is_admin {
                let _ = send(&ServerMessage::Response {
                    id,
                    response: PlanResponse::Error(PlanError::Unauthorized),
                });
                continue;
            }
            let plan_id = *plan_id;
            let version = version.clone();
            // Save current plan as a new snapshot so the restore can be undone.
            {
                let eng_guard = engine.lock().unwrap();
                if let Some(eng) = eng_guard.as_ref() {
                    let _ = storage.lock().unwrap().save(eng.plan());
                }
            }
            let load_result = storage.lock().unwrap().load_version(plan_id, &version);
            match load_result {
                Ok(restored_plan) => {
                    let restored_clone = {
                        let mut eng_guard = engine.lock().unwrap();
                        *eng_guard = Some(PlanEngine::new(restored_plan));
                        let eng_ref = eng_guard.as_ref().unwrap();
                        let _ = storage.lock().unwrap().save(eng_ref.plan());
                        eng_ref.plan().clone()
                    };
                    let has_monday = storage
                        .lock()
                        .unwrap()
                        .load_monday_config(restored_clone.id)
                        .is_some();
                    if send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::PlanUpdated,
                    })
                    .is_err()
                    {
                        break;
                    }
                    if send(&ServerMessage::PlanState {
                        plan: Box::new(restored_clone.clone()),
                        has_monday_integration: has_monday,
                    })
                    .is_err()
                    {
                        break;
                    }
                    broadcast_plan_state(session_id, &registry, &restored_clone, has_monday);
                }
                Err(e) => {
                    eprintln!("[{peer}] restore error: {e}");
                    let _ = send(&ServerMessage::Response {
                        id,
                        response: PlanResponse::Error(PlanError::AuthError(format!(
                            "Failed to restore version: {e}"
                        ))),
                    });
                }
            }
            continue;
        }
        // ── End auth PlanRequest handlers ───────────────────────────────────

        let response = {
            let mut eng_guard = engine.lock().unwrap();
            if let Some(eng) = eng_guard.as_mut() {
                eng.apply_request(request)
            } else {
                PlanResponse::Error(PlanError::NoPlanActive)
            }
        };
        let plan_changed = matches!(response, PlanResponse::PlanUpdated);
        if send(&ServerMessage::Response { id, response }).is_err() {
            break;
        }
        if plan_changed {
            let changed = {
                let eng_guard = engine.lock().unwrap();
                if let Some(eng) = eng_guard.as_ref() {
                    let _ = storage.lock().unwrap().save(eng.plan());
                    let plan = eng.plan().clone();
                    let has_monday = storage
                        .lock()
                        .unwrap()
                        .load_monday_config(plan.id)
                        .is_some();
                    Some((plan, has_monday))
                } else {
                    None
                }
            };
            if let Some((changed_plan, has_monday_integration)) = changed {
                if send(&ServerMessage::PlanState {
                    plan: Box::new(changed_plan.clone()),
                    has_monday_integration,
                })
                .is_err()
                {
                    break;
                }
                broadcast_plan_state(session_id, &registry, &changed_plan, has_monday_integration);
            }
        }
    }
    eprintln!("[disconnect] {peer}: disconnected (served {request_count} requests)");
}
