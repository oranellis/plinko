// modules {{{
use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::NaiveDate;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use plinko::{
    codec,
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
        user::{NewTenantLoginUser, Role, TenantLoginUser},
    },
    net::{actor, http},
    planner::{plan::NewPlan, plan_update::PlanUpdate},
    protocol::{ClientMessage, ServerMessage},
    store::Store,
    types::{crud::Crud, short_id::ShortId, user::NewPlanUser},
};
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};
use tower::ServiceExt;
use uuid::Uuid;
// }}}

// helpers {{{
const PASSWORD: &str = "correct horse";

struct Server {
    app: Router,
    handle: actor::Handle,
    admin: Uuid,
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

async fn server() -> Server {
    server_on(today()).await
}

async fn server_on(day: NaiveDate) -> Server {
    let admin = TenantLoginUser::new(
        "Admin".to_string(),
        "admin@example.com".to_string(),
        Role::Admin,
    );
    let admin_id = *admin.id();
    let tenant = Tenant::new_with_login(ShortId::new(), "Tenant".to_string(), day, admin);
    let store = Arc::new(Store::open_in_memory().unwrap());
    store.create(&tenant).unwrap();
    store.set_password(admin_id, PASSWORD).unwrap();
    let handle = actor::start(store).unwrap();
    let app = http::router(handle.clone(), None, false);
    Server {
        app,
        handle,
        admin: admin_id,
    }
}

async fn call(
    app: &Router,
    request: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

fn post(uri: &str, body: &str) -> axum::http::request::Builder {
    let _ = body;
    Request::post(uri).header(header::CONTENT_TYPE, "application/json")
}

fn login_body(email: &str, password: &str) -> Body {
    Body::from(format!(r#"{{"email":"{email}","password":"{password}"}}"#))
}

fn cookie_of(headers: &axum::http::HeaderMap) -> String {
    let set = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    set.split(';').next().unwrap().to_string()
}
// }}}

// logging_in_gives_a_session_that_logging_out_ends {{{
#[tokio::test]
async fn logging_in_gives_a_session_that_logging_out_ends() {
    let server = server().await;
    let wrong = post("/api/login", "")
        .body(login_body("admin@example.com", "nope"))
        .unwrap();
    assert_eq!(call(&server.app, wrong).await.0, StatusCode::UNAUTHORIZED);
    let unknown = post("/api/login", "")
        .body(login_body("who@example.com", PASSWORD))
        .unwrap();
    assert_eq!(call(&server.app, unknown).await.0, StatusCode::UNAUTHORIZED);

    let login = post("/api/login", "")
        .body(login_body("ADMIN@example.com", PASSWORD))
        .unwrap();
    let (status, headers, body) = call(&server.app, login).await;
    assert_eq!(status, StatusCode::OK);
    let set = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Lax"));
    assert!(String::from_utf8(body).unwrap().contains("Admin"));
    let cookie = cookie_of(&headers);

    let me = |cookie: &str| {
        Request::get("/api/me")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(call(&server.app, me(&cookie)).await.0, StatusCode::OK);
    assert_eq!(
        call(&server.app, me("plinko_session=made-up")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let anonymous = Request::get("/api/me").body(Body::empty()).unwrap();
    assert_eq!(
        call(&server.app, anonymous).await.0,
        StatusCode::UNAUTHORIZED
    );

    let logout = Request::post("/api/logout")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&server.app, logout).await.0, StatusCode::NO_CONTENT);
    assert_eq!(
        call(&server.app, me(&cookie)).await.0,
        StatusCode::UNAUTHORIZED
    );
}
// }}}

// requests_from_other_origins_are_refused {{{
#[tokio::test]
async fn requests_from_other_origins_are_refused() {
    let server = server().await;
    let login = post("/api/login", "")
        .header(header::HOST, "plinko.example.com")
        .header(header::ORIGIN, "https://evil.example.com")
        .body(login_body("admin@example.com", PASSWORD))
        .unwrap();
    assert_eq!(call(&server.app, login).await.0, StatusCode::FORBIDDEN);
    let login = post("/api/login", "")
        .header(header::HOST, "plinko.example.com")
        .header(header::ORIGIN, "https://plinko.example.com")
        .body(login_body("admin@example.com", PASSWORD))
        .unwrap();
    assert_eq!(call(&server.app, login).await.0, StatusCode::OK);
}
// }}}

// passwords_change_for_oneself_with_the_old_one_and_for_others_as_admin {{{
#[tokio::test]
async fn passwords_change_for_oneself_with_the_old_one_and_for_others_as_admin() {
    let server = server().await;
    let login = post("/api/login", "")
        .body(login_body("admin@example.com", PASSWORD))
        .unwrap();
    let cookie = cookie_of(&call(&server.app, login).await.1);
    let change = |cookie: &str, body: &str| {
        post("/api/password", "")
            .header(header::COOKIE, cookie)
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let (status, _, _) = call(
        &server.app,
        change(&cookie, r#"{"current":"wrong one","new":"another one"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = call(
        &server.app,
        change(&cookie, r#"{"current":"correct horse","new":"short"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, headers, _) = call(
        &server.app,
        change(
            &cookie,
            r#"{"current":"correct horse","new":"another one"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // the old session is gone, the response carries a new one
    let me = |cookie: &str| {
        Request::get("/api/me")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        call(&server.app, me(&cookie)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&server.app, me(&cookie_of(&headers))).await.0,
        StatusCode::OK
    );
    let login = post("/api/login", "")
        .body(login_body("admin@example.com", "another one"))
        .unwrap();
    assert_eq!(call(&server.app, login).await.0, StatusCode::OK);
}
// }}}

// helpers {{{
type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn listen(server: &Server) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, server.app.clone()).into_future());
    address
}

async fn connect(address: std::net::SocketAddr, token: &str) -> Result<Socket, tungstenite::Error> {
    let mut request = format!("ws://{address}/ws").into_client_request().unwrap();
    request.headers_mut().insert(
        header::COOKIE,
        format!("plinko_session={token}").parse().unwrap(),
    );
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(socket, _)| socket)
}

async fn next(socket: &mut Socket) -> ServerMessage {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(20), socket.next())
            .await
            .expect("a message arrives")
            .expect("the socket is open")
            .unwrap();
        if let tungstenite::Message::Binary(bytes) = message {
            return codec::decode(&bytes).unwrap();
        }
    }
}

async fn quiet(socket: &mut Socket) -> bool {
    tokio::time::timeout(Duration::from_millis(300), socket.next())
        .await
        .is_err()
}

async fn send(socket: &mut Socket, request: u32, update: TenantUpdateType) {
    let bytes = codec::encode(&ClientMessage::Update { request, update }).unwrap();
    socket
        .send(tungstenite::Message::Binary(bytes.into()))
        .await
        .unwrap();
}

/// Reads until the answer to `request`: its sequence number or why not, and what came before it
async fn answer(socket: &mut Socket, request: u32) -> (Result<u64, String>, Vec<ServerMessage>) {
    let mut before = Vec::new();
    loop {
        match next(socket).await {
            ServerMessage::Ack {
                request: r, seq, ..
            } if r == request => return (Ok(seq), before),
            ServerMessage::Rejected { request: r, error } if r == request => {
                return (Err(error), before);
            }
            other => before.push(other),
        }
    }
}
// }}}

// the_socket_needs_a_session_and_starts_with_a_snapshot {{{
#[tokio::test]
async fn the_socket_needs_a_session_and_starts_with_a_snapshot() {
    let server = server().await;
    let address = listen(&server).await;
    assert!(connect(address, "made-up").await.is_err());
    let token = server.handle.store.start_session(server.admin).unwrap();
    let mut socket = connect(address, &token).await.unwrap();
    let ServerMessage::Snapshot { seq, user, tenant } = next(&mut socket).await else {
        panic!("a snapshot comes first");
    };
    assert_eq!((seq, user, tenant.today()), (0, server.admin, today()));
    // then the schedule, which exists once the scheduling thread has run
    assert!(matches!(
        next(&mut socket).await,
        ServerMessage::Schedule { seq: 0, .. }
    ));
}
// }}}

// updates_are_applied_in_order_and_shared_with_those_who_may_see_them {{{
#[tokio::test]
async fn updates_are_applied_in_order_and_shared_with_those_who_may_see_them() {
    let server = server().await;
    let address = listen(&server).await;
    let token = server.handle.store.start_session(server.admin).unwrap();
    let mut admin = connect(address, &token).await.unwrap();
    assert!(matches!(
        next(&mut admin).await,
        ServerMessage::Snapshot { .. }
    ));

    // a member, who sees no plans yet
    let new = NewTenantLoginUser {
        id: Uuid::nil(),
        name: "Member".to_string(),
        email: "member@example.com".to_string(),
        role: Role::Member,
    };
    send(&mut admin, 1, TenantUpdateType::User(Crud::Add(new))).await;
    assert_eq!(answer(&mut admin, 1).await.0, Ok(1));
    let member = server
        .handle
        .user_by_email("member@example.com".to_string())
        .await
        .unwrap();
    assert_ne!(member, Uuid::nil(), "the server gave it an id");
    let member_token = server.handle.store.start_session(member).unwrap();
    let mut other = connect(address, &member_token).await.unwrap();
    let ServerMessage::Snapshot { tenant, .. } = next(&mut other).await else {
        panic!("a snapshot comes first");
    };
    assert_eq!(tenant.users().len(), 2);
    assert!(tenant.plans().is_empty());

    // the admin makes a plan: the member is told nothing about it
    let plan = NewPlan {
        id: ShortId::new(),
        name: "Secret".to_string(),
    };
    send(
        &mut admin,
        2,
        TenantUpdateType::Plan(PlanTenantUpdate::Add(plan)),
    )
    .await;
    let (seq, before) = answer(&mut admin, 2).await;
    assert_eq!(seq, Ok(2));
    let plan = before
        .iter()
        .find_map(|m| match m {
            ServerMessage::Snapshot { tenant, .. } => tenant.plans().keys().next().copied(),
            _ => None,
        })
        .expect("the admin is sent the plan");
    while !matches!(
        next(&mut other).await,
        ServerMessage::Schedule { seq: 2, .. }
    ) {}
    assert!(quiet(&mut other).await);

    // a refused update is answered as such and changes nothing
    send(&mut other, 7, TenantUpdateType::SetToday(today())).await;
    assert!(answer(&mut other, 7).await.0.is_err());

    // adding the member to the plan shows it to them, as a snapshot since what they see changed
    let update = PlanUpdate::User(Crud::Add(NewPlanUser { id: member }));
    let update = TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, Box::new(update)));
    send(&mut admin, 3, update).await;
    assert_eq!(answer(&mut admin, 3).await.0, Ok(3));
    loop {
        if let ServerMessage::Snapshot { seq, tenant, .. } = next(&mut other).await {
            assert_eq!(seq, 3);
            assert_eq!(tenant.plans()[&plan].users().len(), 2);
            break;
        }
    }
}
// }}}

// a_burst_of_updates_ends_with_a_schedule_for_the_last {{{
#[tokio::test]
async fn a_burst_of_updates_ends_with_a_schedule_for_the_last() {
    let server = server().await;
    let address = listen(&server).await;
    let token = server.handle.store.start_session(server.admin).unwrap();
    let mut socket = connect(address, &token).await.unwrap();
    for request in 1..=10 {
        let new = NewPlan {
            id: ShortId::new(),
            name: "Plan".to_string(),
        };
        let add = TenantUpdateType::Plan(PlanTenantUpdate::Add(new));
        send(&mut socket, request, add).await;
    }
    assert_eq!(answer(&mut socket, 10).await.0, Ok(10));
    // the schedule can trail, but it gets there
    while !matches!(
        next(&mut socket).await,
        ServerMessage::Schedule { seq: 10, .. }
    ) {}
}
// }}}

// the_server_sets_the_clock_to_today_and_nobody_else_does {{{
#[tokio::test]
async fn the_server_sets_the_clock_to_today_and_nobody_else_does() {
    // it starts on another day, which the server corrects as it starts and is not left to a client
    let server = server_on(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()).await;
    let address = listen(&server).await;
    let token = server.handle.store.start_session(server.admin).unwrap();
    let mut socket = connect(address, &token).await.unwrap();
    send(&mut socket, 1, TenantUpdateType::SetToday(today())).await;
    assert!(answer(&mut socket, 1).await.0.is_err());
    // the saved tenant is on today before long
    for _ in 0..100 {
        if server.handle.store.load().unwrap().0.today() == today() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the clock was not set");
}
// }}}
