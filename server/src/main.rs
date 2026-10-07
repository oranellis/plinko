//! The server of one tenant. Settings come from the environment:
//! `PLINKO_DB` (default `tenant.db`), `PLINKO_LISTEN` (default `127.0.0.1:3000`), `PLINKO_FILES`
//! (the client build, default `client/dist`), `PLINKO_INSECURE_COOKIES=1` for plain HTTP, and on
//! the first run `PLINKO_TENANT`, `PLINKO_ADMIN_EMAIL` and `PLINKO_ADMIN_PASSWORD`.

use std::{path::PathBuf, sync::Arc};

use plinko::{
    multitenancy::{
        tenant::Tenant,
        user::{Role, TenantLoginUser},
    },
    net::{actor, http},
    store::Store,
    types::short_id::ShortId,
};

fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn required(name: &str) -> String {
    setting(name).unwrap_or_else(|| panic!("a new tenant needs {name}"))
}

#[tokio::main]
async fn main() {
    let db = PathBuf::from(setting("PLINKO_DB").unwrap_or("tenant.db".to_string()));
    let store = Arc::new(Store::open(&db).expect("the database opens"));
    if store.is_empty().expect("the database reads") {
        let admin = TenantLoginUser::new(
            "Admin".to_string(),
            required("PLINKO_ADMIN_EMAIL"),
            Role::Admin,
        );
        let id = *admin.id();
        let today = chrono::Utc::now().date_naive();
        let name = required("PLINKO_TENANT");
        let tenant = Tenant::new_with_login(ShortId::new(), name, today, admin);
        store.create(&tenant).expect("the tenant saves");
        store
            .set_password(id, &required("PLINKO_ADMIN_PASSWORD"))
            .expect("the password saves");
    }
    let handle = actor::start(store).expect("the tenant loads");
    let files = PathBuf::from(setting("PLINKO_FILES").unwrap_or("client/dist".to_string()));
    let secure = setting("PLINKO_INSECURE_COOKIES").is_none();
    let app = http::router(handle, files.is_dir().then_some(files), secure);
    let listen = setting("PLINKO_LISTEN").unwrap_or("127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .expect("the address binds");
    eprintln!("listening on {listen}");
    axum::serve(listener, app).await.expect("the server runs");
}
