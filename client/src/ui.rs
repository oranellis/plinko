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
                    <button type="button" class="plain" aria-label="Close" on:click=move |_| on_close.run(())>"✕"</button>
                </header>
                <div class="body">{children()}</div>
            </div>
        </div>
    }
}

#[component]
pub fn Field(#[prop(into)] label: Signal<String>, children: Children) -> impl IntoView {
    view! {
        <label class="field">
            <span>{move || label.get()}</span>
            {children()}
        </label>
    }
}

/// A labelled group of controls, for the ones a label cannot wrap
#[component]
pub fn Block(#[prop(into)] label: String, children: Children) -> impl IntoView {
    view! {
        <div class="field">
            <span>{label}</span>
            {children()}
        </div>
    }
}

/// Buttons side by side, one of which is picked
#[component]
pub fn Segmented(
    #[prop(into)] value: Signal<String>,
    options: Signal<Vec<(String, String)>>,
    on_change: Callback<String>,
) -> impl IntoView {
    view! {
        <div class="segmented">
            {move || {
                options
                    .get()
                    .into_iter()
                    .map(|(v, label)| {
                        let mine = v.clone();
                        view! {
                            <button
                                type="button"
                                class:on=move || value.get() == mine
                                on:click=move |_| on_change.run(v.clone())
                            >
                                {label}
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </div>
    }
}

/// A bordered list of rows with an add button under it
#[component]
pub fn List(
    #[prop(into)] label: String,
    #[prop(into)] empty: String,
    #[prop(into)] add: String,
    #[prop(into)] count: Signal<usize>,
    on_add: Callback<()>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="field">
            <span>{label}</span>
            <div class="list">
                <Show when=move || count.get() == 0>
                    <p class="none">{empty.clone()}</p>
                </Show>
                {children()}
            </div>
            <button type="button" class="add" on:click=move |_| on_add.run(())>{format!("+ {add}")}</button>
        </div>
    }
}

/// The buttons under a form: an optional delete on the left, then cancel and save
#[component]
pub fn Actions(
    on_cancel: Callback<()>,
    #[prop(optional)] on_delete: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="actions">
            {on_delete.map(|d| view! { <button type="button" class="danger" on:click=move |_| d.run(())>"Delete"</button> })}
            <span class="grow"></span>
            <button type="button" on:click=move |_| on_cancel.run(())>"Cancel"</button>
            <button type="submit" class="primary">"Save"</button>
        </div>
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
    let kind = if kind.is_empty() {
        "text".to_string()
    } else {
        kind
    };
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
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect();
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
