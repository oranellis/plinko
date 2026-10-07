mod api;
mod availability;
mod editor;
mod format;
mod forms;
mod gantt;
mod login;
mod model;
mod nav;
mod people;
mod plan;
mod plans;
mod resources;
mod settings;
mod shell;
mod sync;
mod ui;
mod work;

use leptos::{prelude::*, task::spawn_local};
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{login::Login, shell::Shell, sync::Sync};

#[component]
fn App() -> impl IntoView {
    let sync = Sync::new();
    provide_context(sync);
    let booted = RwSignal::new(false);
    spawn_local(async move {
        if let Some(profile) = api::me().await {
            sync.me.set(Some(profile));
            sync.connect();
        }
        booted.set(true);
    });
    view! {
        <Show when=move || booted.get() fallback=|| view! { <p class="loading">"Loading…"</p> }>
            <Show when=move || sync.me.with(Option::is_some) fallback=|| view! { <Login /> }>
                <Show
                    when=move || sync.tenant.with(Option::is_some)
                    fallback=|| view! { <p class="loading">"Loading your plans…"</p> }
                >
                    <Shell />
                </Show>
            </Show>
        </Show>
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
