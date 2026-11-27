//! The HTTP side: logging in and out, and passwords. Everything else goes over the socket.

// modules {{{
use gloo_net::http::Request;
use plinko::protocol::Profile;
use serde::Serialize;
use uuid::Uuid;
// }}}

// request types {{{
#[derive(Serialize)]
struct Login<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct ChangePassword<'a> {
    user: Option<Uuid>,
    current: Option<&'a str>,
    new: &'a str,
}
// }}}

// helpers {{{
pub async fn login(email: &str, password: &str) -> Result<Profile, String> {
    let request = Request::post("/api/login")
        .json(&Login { email, password })
        .map_err(|e| e.to_string())?;
    let response = request.send().await.map_err(|e| e.to_string())?;
    match response.status() {
        200 => response.json().await.map_err(|e| e.to_string()),
        401 => Err("The email or password is wrong".to_string()),
        _ => Err("The server could not log you in".to_string()),
    }
}

/// Who is logged in: `Ok(None)` if no one is, `Err` if the server could not be asked
pub async fn me() -> Result<Option<Profile>, ()> {
    let response = Request::get("/api/me").send().await.map_err(|_| ())?;
    match response.status() {
        200 => response.json().await.map(Some).map_err(|_| ()),
        401 | 403 => Ok(None),
        _ => Err(()),
    }
}

pub async fn logout() {
    let _ = Request::post("/api/logout").send().await;
}

/// Changes your own password given the current one, or anyone's as an admin with no `current`
pub async fn change_password(
    user: Option<Uuid>,
    current: Option<&str>,
    new: &str,
) -> Result<(), String> {
    let request = Request::post("/api/password")
        .json(&ChangePassword { user, current, new })
        .map_err(|e| e.to_string())?;
    let response = request.send().await.map_err(|e| e.to_string())?;
    match response.status() {
        204 => Ok(()),
        400 => Err("The password must be at least 8 characters".to_string()),
        403 => Err("That is not allowed, check the current password".to_string()),
        _ => Err("The password could not be changed".to_string()),
    }
}
// }}}
