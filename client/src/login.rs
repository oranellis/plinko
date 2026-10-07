use leptos::{prelude::*, task::spawn_local};

use crate::{api, ui::{Field, Text, use_sync}};

#[component]
pub fn Login() -> impl IntoView {
    let sync = use_sync();
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let failure = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        spawn_local(async move {
            match api::login(&email.get_untracked(), &password.get_untracked()).await {
                Ok(profile) => {
                    sync.me.set(Some(profile));
                    sync.connect();
                }
                Err(why) => failure.set(Some(why)),
            }
            busy.set(false);
        });
    };
    view! {
        <main class="login">
            <form on:submit=submit>
                <h1>"Plinko"</h1>
                <Field label="Email">
                    <Text value=email on_input=Callback::new(move |v| email.set(v)) kind="email" />
                </Field>
                <Field label="Password">
                    <Text value=password on_input=Callback::new(move |v| password.set(v)) kind="password" />
                </Field>
                <Show when=move || failure.get().is_some()>
                    <p class="error" role="alert">{move || failure.get()}</p>
                </Show>
                <button type="submit" class="primary" disabled=move || busy.get()>"Log in"</button>
            </form>
        </main>
    }
}
