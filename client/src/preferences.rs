//! The signed-in person's name, password and date display.

// modules {{{
use leptos::{prelude::*, task::spawn_local};
use plinko::{
    common::crud::Crud,
    multitenancy::{tenant_update::TenantUpdateType, user::TenantLoginUserPatch},
};

use crate::{
    api,
    format::{DateFormat, WeekStart, use_preferences},
    nav::{Availability, Route},
    ui::{Back, Field, Select, Text, use_sync},
};
// }}}

// Preferences {{{
#[component]
pub fn Preferences() -> impl IntoView {
    view! {
        <div class="people">
            <Back to=Route::Home />
            <h1>"Preferences"</h1>
            <Profile />
            <Password />
            <Display />
        </div>
    }
}
// }}}

// Profile {{{
#[component]
fn Profile() -> impl IntoView {
    let sync = use_sync();
    let me = sync.me.get_untracked().map(|m| m.id).unwrap_or_default();
    let (old_name, old_email) = sync
        .tenant
        .with_untracked(|t| {
            let user = t.as_ref()?.users().get(&me)?;
            Some((user.name().to_string(), user.email().to_string()))
        })
        .unwrap_or_default();
    let name = RwSignal::new(old_name.clone());
    let email = RwSignal::new(old_email.clone());
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let (name, email) = (name.get_untracked(), email.get_untracked());
        let patch = TenantLoginUserPatch {
            id: me,
            name: (name != old_name && !name.trim().is_empty()).then_some(name),
            email: (email != old_email && !email.trim().is_empty()).then_some(email),
            role: None,
        };
        if patch.name.is_some() || patch.email.is_some() {
            sync.send(TenantUpdateType::User(Crud::Update(patch)));
        }
    };
    view! {
        <section>
            <h2>"Profile"</h2>
            <form class="form" on:submit=save>
                <Field label="Display Name">
                    <Text value=name on_input=Callback::new(move |v| name.set(v)) />
                </Field>
                <Field label="Email">
                    <Text value=email on_input=Callback::new(move |v| email.set(v)) kind="email" />
                </Field>
                <button type="submit" class="primary">"Save Profile"</button>
            </form>
        </section>
    }
}
// }}}

// Password {{{
#[component]
fn Password() -> impl IntoView {
    let current = RwSignal::new(String::new());
    let new = RwSignal::new(String::new());
    let again = RwSignal::new(String::new());
    let result = RwSignal::new(None::<Result<(), String>>);
    let change = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if new.get_untracked() != again.get_untracked() {
            result.set(Some(Err("The new passwords do not match".to_string())));
            return;
        }
        spawn_local(async move {
            let done =
                api::change_password(None, Some(&current.get_untracked()), &new.get_untracked())
                    .await;
            if done.is_ok() {
                current.set(String::new());
                new.set(String::new());
                again.set(String::new());
            }
            result.set(Some(done));
        });
    };
    view! {
        <section>
            <h2>"Change Password"</h2>
            <form class="form" on:submit=change>
                <Field label="Current Password">
                    <Text value=current on_input=Callback::new(move |v| current.set(v)) kind="password" />
                </Field>
                <Field label="New Password">
                    <Text value=new on_input=Callback::new(move |v| new.set(v)) kind="password" />
                </Field>
                <Field label="Confirm New Password">
                    <Text value=again on_input=Callback::new(move |v| again.set(v)) kind="password" />
                </Field>
                {move || match result.get() {
                    Some(Err(why)) => view! { <p class="error">{why}</p> }.into_any(),
                    Some(Ok(())) => view! { <p class="small">"Password changed"</p> }.into_any(),
                    None => ().into_any(),
                }}
                <button type="submit" class="primary">"Change Password"</button>
            </form>
        </section>
    }
}
// }}}

// Display {{{
#[component]
fn Display() -> impl IntoView {
    let sync = use_sync();
    let prefs = use_preferences();
    let me = sync.me.get_untracked().map(|m| m.id).unwrap_or_default();
    view! {
        <section>
            <h2>"Display"</h2>
            <Field label="Date Format">
                <Select
                    value=Signal::derive(move || prefs.date_format.get().label().to_string())
                    options=Signal::stored(
                        DateFormat::ALL.map(|f| (f.label().to_string(), f.label().to_string())).to_vec(),
                    )
                    on_change=Callback::new(move |v: String| {
                        if let Some(f) = DateFormat::ALL.into_iter().find(|f| f.label() == v) {
                            prefs.date_format.set(f);
                        }
                    })
                />
            </Field>
            <Field label="Week Starts On">
                <Select
                    value=Signal::derive(move || prefs.week_start.get().label().to_string())
                    options=Signal::stored(
                        WeekStart::ALL.map(|w| (w.label().to_string(), w.label().to_string())).to_vec(),
                    )
                    on_change=Callback::new(move |v: String| {
                        if let Some(w) = WeekStart::ALL.into_iter().find(|w| w.label() == v) {
                            prefs.week_start.set(w);
                        }
                    })
                />
            </Field>
            <a href=Route::Availability(Availability::User(me)).path()>"Edit my availability"</a>
        </section>
    }
}
// }}}
