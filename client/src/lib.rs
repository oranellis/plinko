mod allocations;
mod api;
mod availability;
mod card;
mod chart_edit;
mod context;
mod data;
mod datepicker;
mod draft;
mod editor;
mod escape;
mod format;
mod forms;
mod gantt;
mod glide;
mod guard;
mod help;
mod home;
mod icons;
mod login;
mod mine;
mod model;
mod nav;
mod people;
mod plan;
mod plans;
mod preferences;
mod rulecalendar;
mod settings;
mod shell;
mod sync;
mod ui;

use leptos::{prelude::*, task::spawn_local};
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{login::Login, shell::Shell, sync::Sync};

#[component]
fn App() -> impl IntoView {
    let sync = Sync::new();
    escape::listen();
    provide_context(sync);
    provide_context(nav::Reveal(RwSignal::new(None)));
    format::provide_preferences();
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
