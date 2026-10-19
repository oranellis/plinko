//! HTTP: logging in and out, the WebSocket, and the client's files. Sessions are a cookie, so
//! the browser sends it with the WebSocket as well. Anything that changes state or opens the
//! socket must come from the page's own origin.

// modules {{{
use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use crate::{
    codec,
    net::actor::{Command, Handle},
    protocol::{ClientMessage, Profile},
    store::{Store, auth::SESSION_LIFETIME},
};
// }}}

// constants {{{
const COOKIE: &str = "plinko_session";
const MIN_PASSWORD_LENGTH: usize = 8;
// }}}

#[derive(Clone)]
struct AppState {
    handle: Handle,
    /// The session cookie is only sent over HTTPS, unless this is a local test
    secure_cookies: bool,
}

// fn router {{{
/// The routes, serving the client from `files` if given
pub fn router(handle: Handle, files: Option<PathBuf>, secure_cookies: bool) -> Router {
    let state = AppState {
        handle,
        secure_cookies,
    };
    let app = Router::new()
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/api/password", post(password))
        .route("/ws", get(socket))
        .with_state(state);
    match files {
        Some(dir) => {
            let index = ServeFile::new(dir.join("index.html"));
            app.fallback_service(ServeDir::new(dir).fallback(index))
        }
        None => app,
    }
}
// }}}

// helpers {{{
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        // not a browser
        return true;
    };
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    let origin = origin.to_str().ok().and_then(|o| o.split("://").nth(1));
    host.is_some() && host == origin
}

fn token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    let found = cookies
        .split(';')
        .find_map(|c| c.trim().strip_prefix(&format!("{COOKIE}=")));
    found.map(str::to_string)
}

async fn user_of(state: &AppState, headers: &HeaderMap) -> Option<Uuid> {
    let token = token(headers)?;
    state.handle.store.session_user(&token).ok()?
}

fn session_cookie(state: &AppState, token: &str, lifetime: u64) -> (HeaderName, HeaderValue) {
    let secure = if state.secure_cookies { "; Secure" } else { "" };
    let cookie =
        format!("{COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={lifetime}{secure}");
    let value = HeaderValue::from_str(&cookie).expect("a cookie is plain text");
    (header::SET_COOKIE, value)
}

/// Runs the store call off the async threads, since hashing a password takes a while
async fn blocking<T: Send + 'static>(
    store: &Arc<Store>,
    call: impl FnOnce(&Store) -> crate::types::error::PlinkoResult<T> + Send + 'static,
) -> Option<T> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || call(&store).ok())
        .await
        .ok()?
}
// }}}

// AppState supporting types {{{
#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}
// }}}

// fn login {{{
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(login): Json<Login>,
) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let user = state.handle.user_by_email(login.email).await;
    let store = state.handle.store.clone();
    let checked = blocking(&store, move |s| {
        s.check_password(user.unwrap_or_default(), &login.password)
    })
    .await;
    let (Some(user), Some(true)) = (user, checked) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(token) = blocking(&store, move |s| s.start_session(user)).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let cookie = session_cookie(&state, &token, SESSION_LIFETIME.as_secs());
    match state.handle.profile(user).await {
        Some(profile) => ([cookie], Json(profile)).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}
// }}}

// fn logout {{{
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(token) = token(&headers) {
        let _ = state.handle.store.end_session(&token);
    }
    ([session_cookie(&state, "", 0)], StatusCode::NO_CONTENT).into_response()
}
// }}}

// fn me {{{
async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(user) = user_of(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.handle.profile(user).await {
        Some(profile) => Json::<Profile>(profile).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}
// }}}

// AppState supporting types {{{
#[derive(Deserialize)]
struct ChangePassword {
    /// Whose password, the caller's own if left out
    user: Option<Uuid>,
    /// The caller's current password, needed to change their own
    current: Option<String>,
    new: String,
}
// }}}

// fn password {{{
/// Anyone may change their own password given the current one, an admin may set anyone's
async fn password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(change): Json<ChangePassword>,
) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(me) = user_of(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if change.new.chars().count() < MIN_PASSWORD_LENGTH {
        return (StatusCode::BAD_REQUEST, "the password is too short").into_response();
    }
    let target = change.user.unwrap_or(me);
    let admin = state.handle.profile(me).await.is_some_and(|p| p.admin);
    let store = state.handle.store.clone();
    if target == me {
        let current = change.current.unwrap_or_default();
        let checked = blocking(&store, move |s| s.check_password(me, &current)).await;
        if checked != Some(true) {
            return StatusCode::FORBIDDEN.into_response();
        }
    } else if !admin || state.handle.profile(target).await.is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let new = change.new;
    let set = blocking(&store, move |s| {
        s.set_password(target, &new)?;
        s.end_sessions_of(target)
    })
    .await;
    if set.is_none() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if target != me {
        return StatusCode::NO_CONTENT.into_response();
    }
    // changing your own password ends every session, so this one starts again
    match blocking(&store, move |s| s.start_session(me)).await {
        Some(token) => {
            let cookie = session_cookie(&state, &token, SESSION_LIFETIME.as_secs());
            ([cookie], StatusCode::NO_CONTENT).into_response()
        }
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
// }}}

// fn socket {{{
async fn socket(
    State(state): State<AppState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(user) = user_of(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    upgrade.on_upgrade(move |socket| serve(state.handle, user, socket))
}
// }}}

// fn serve {{{
/// Sends a client what the actor says and gives the actor what the client asks for
async fn serve(handle: Handle, user: Uuid, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (out, mut outgoing) = mpsc::unbounded_channel();
    let Some(subscriber) = handle.subscribe(user, out).await else {
        return;
    };
    let sending = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            let Ok(bytes) = codec::encode(&message) else {
                continue;
            };
            if sink.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Binary(bytes) => {
                let Ok(ClientMessage::Update { request, update }) = codec::decode(&bytes) else {
                    break;
                };
                let command = Command::Update {
                    subscriber,
                    request,
                    update,
                };
                if handle.commands.send(command).is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    let _ = handle.commands.send(Command::Unsubscribe(subscriber));
    sending.abort();
}
// }}}
