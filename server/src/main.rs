//! The server of one tenant. Settings come from the environment:
//! `PLINKO_TENANT` (the tenant's name, always needed, its files are in `data/<name>` under the
//! directory the server runs from), `PLINKO_LISTEN` (default `127.0.0.1:3000`), `PLINKO_FILES`
//! (the client build, default `client/dist`), `PLINKO_INSECURE_COOKIES=1` for plain HTTP, and on
//! the first run `PLINKO_ADMIN_EMAIL` and `PLINKO_ADMIN_PASSWORD`.

use std::{path::PathBuf, sync::Arc};

use plinko::{
    multitenancy::{
        tenant::Tenant,
        user::{Role, TenantLoginUser},
    },
    net::{actor, http},
    store::{self, Store},
    types::short_id::ShortId,
};

fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn required(name: &str) -> String {
    setting(name).unwrap_or_else(|| panic!("{name} is needed"))
}

#[tokio::main]
async fn main() {
    let name = required("PLINKO_TENANT");
    let dir = store::tenant_dir(&name).unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir_all(&dir).expect("the tenant's folder is made");
    let store = Arc::new(Store::open(&dir.join("tenant.db")).expect("the database opens"));
    if store.is_empty().expect("the database reads") {
        let admin = TenantLoginUser::new(
            "Admin".to_string(),
            required("PLINKO_ADMIN_EMAIL"),
            Role::Admin,
        );
        let id = *admin.id();
        let today = chrono::Utc::now().date_naive();
        let tenant = Tenant::new_with_login(ShortId::new(), name, today, admin);
        store.create(&tenant).expect("the tenant saves");
        store
            .set_password(id, &required("PLINKO_ADMIN_PASSWORD"))
            .expect("the password saves");
    }
    let handle = actor::start(store).expect("the tenant loads");
    // a tenant that was down over midnight catches up on its clock
    let _ = handle.commands.send(actor::Command::Tick);
    let files = PathBuf::from(setting("PLINKO_FILES").unwrap_or("client/dist".to_string()));
    let secure = setting("PLINKO_INSECURE_COOKIES").is_none();
    let app = http::router(handle, files.is_dir().then_some(files), secure);
    let listen = setting("PLINKO_LISTEN").unwrap_or("127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .expect("the address binds");
    eprintln!("listening on http://{listen}");
    axum::serve(listener, app).await.expect("the server runs");
}
