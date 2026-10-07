//! Small pieces the pages share.

use leptos::prelude::*;

use crate::sync::{Link, Sync};

pub fn use_sync() -> Sync {
    expect_context::<Sync>()
}

/// A panel over the page, a sheet from the bottom on a phone
#[component]
pub fn Modal(
    #[prop(into)] title: String,
    on_close: Callback<()>,
    children: Children,
) -> impl IntoView {
    let label = title.clone();
    view! {
        <div class="backdrop" on:click=move |_| on_close.run(())>
            <div class="modal" role="dialog" aria-label=label on:click=|ev| ev.stop_propagation()>
                <header>
                    <h2>{title}</h2>
                    <button class="plain" aria-label="Close" on:click=move |_| on_close.run(())>"✕"</button>
                </header>
                <div class="body">{children()}</div>
            </div>
        </div>
    }
}

#[component]
pub fn Field(#[prop(into)] label: String, children: Children) -> impl IntoView {
    view! {
        <label class="field">
            <span>{label}</span>
            {children()}
        </label>
    }
}

/// A text box that tells its owner when it changes
#[component]
pub fn Text(
    #[prop(into)] value: Signal<String>,
    on_input: Callback<String>,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] kind: String,
) -> impl IntoView {
    let kind = if kind.is_empty() { "text".to_string() } else { kind };
    view! {
        <input
            type=kind
            placeholder=placeholder
            prop:value=move || value.get()
            on:input=move |ev| on_input.run(event_target_value(&ev))
        />
    }
}

/// A box for a whole number
#[component]
pub fn Number(
    #[prop(into)] value: Signal<i64>,
    on_input: Callback<i64>,
    #[prop(optional)] min: Option<i64>,
    #[prop(optional)] max: Option<i64>,
) -> impl IntoView {
    view! {
        <input
            type="number"
            inputmode="numeric"
            min=min.map(|m| m.to_string())
            max=max.map(|m| m.to_string())
            prop:value=move || value.get().to_string()
            on:input=move |ev| {
                if let Ok(n) = event_target_value(&ev).trim().parse::<i64>() {
                    on_input.run(n);
                }
            }
        />
    }
}

/// A choice of `options` (value and label), holding the value that is picked
#[component]
pub fn Select(
    #[prop(into)] value: Signal<String>,
    options: Signal<Vec<(String, String)>>,
    on_change: Callback<String>,
) -> impl IntoView {
    view! {
        <select on:change=move |ev| on_change.run(event_target_value(&ev)) prop:value=move || value.get()>
            {move || {
                let current = value.get();
                options
                    .get()
                    .into_iter()
                    .map(|(v, label)| {
                        let selected = v == current;
                        view! { <option value=v selected=selected>{label}</option> }
                    })
                    .collect_view()
            }}
        </select>
    }
}

pub fn options<const N: usize>(pairs: [(&str, &str); N]) -> Signal<Vec<(String, String)>> {
    let pairs: Vec<(String, String)> = pairs.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect();
    Signal::stored(pairs)
}

/// How the connection and the schedule are doing
#[component]
pub fn Status() -> impl IntoView {
    let sync = use_sync();
    view! {
        <span class="status" class:warn=move || sync.link.get() != Link::Live>
            {move || match sync.link.get() {
                Link::Connecting => "Connecting…",
                Link::Down => "Offline, reconnecting…",
                Link::Live if sync.recalculating() => "Recalculating…",
                Link::Live => "Up to date",
            }}
        </span>
    }
}
