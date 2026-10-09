//! Small pieces the pages share.

use leptos::{html, prelude::*};

use crate::sync::{Link, Sync};

pub fn use_sync() -> Sync {
    expect_context::<Sync>()
}

/// A panel over the page, a sheet from the bottom on a phone
#[component]
pub fn Modal(
    #[prop(into)] title: Signal<String>,
    on_close: Callback<()>,
    /// A big window, for a calendar
    #[prop(optional)]
    wide: bool,
    children: Children,
) -> impl IntoView {
    let label = move || title.get();
    view! {
        <div class="backdrop" on:click=move |_| on_close.run(())>
            <div class="modal" class:wide=wide role="dialog" aria-label=label on:click=|ev| ev.stop_propagation()>
                <header>
                    <h2>{move || title.get()}</h2>
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

/// One thing a `Picker` can pick. `tone` is a CSS class that colours it: `start` or `target`.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub tone: &'static str,
}

impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<String>, tone: &'static str) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            tone,
        }
    }
}

/// A button that opens a searchable list of `choices`, holding the value that is picked
#[component]
pub fn Picker(
    #[prop(into)] value: Signal<String>,
    #[prop(into)] choices: Signal<Vec<Choice>>,
    on_change: Callback<String>,
    #[prop(optional, into)] placeholder: String,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let filter = RwSignal::new(String::new());
    let anchor = RwSignal::new((0.0, 0.0, 0.0));
    let button = NodeRef::<html::Button>::new();
    let search = NodeRef::<html::Input>::new();
    let current = Memo::new(move |_| {
        let v = value.get();
        choices.with(|all| all.iter().find(|c| c.value == v).cloned())
    });
    let show = move |_| {
        if let Some(b) = button.get_untracked() {
            let r = b.get_bounding_client_rect();
            anchor.set((r.left(), r.bottom() + 2.0, r.width()));
        }
        filter.set(String::new());
        open.set(true);
        set_timeout(
            move || {
                if let Some(i) = search.get_untracked() {
                    let _ = i.focus();
                }
            },
            std::time::Duration::from_millis(0),
        );
    };
    let shown = Memo::new(move |_| {
        let wanted = filter.get().to_lowercase();
        choices.with(|all| {
            all.iter()
                .filter(|c| wanted.is_empty() || c.label.to_lowercase().contains(&wanted))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    view! {
        <button type="button" class="picker" node_ref=button on:click=show>
            {move || match current.get() {
                Some(c) => view! { <span class=c.tone>{c.label}</span> }.into_any(),
                None => view! { <span class="muted">{placeholder.clone()}</span> }.into_any(),
            }}
            <span class="chevron">"▾"</span>
        </button>
        <Show when=move || open.get()>
            <div class="picker-backdrop" on:click=move |_| open.set(false)></div>
            <div
                class="popover"
                style=move || {
                    let (x, y, w) = anchor.get();
                    format!("left:{x}px;top:{y}px;width:{w}px")
                }
            >
                <input
                    class="search"
                    node_ref=search
                    placeholder="Search…"
                    prop:value=move || filter.get()
                    on:input=move |ev| filter.set(event_target_value(&ev))
                    on:keydown=move |ev| if ev.key() == "Escape" { open.set(false) }
                />
                <ul>
                    <For each=move || shown.get() key=|c| c.value.clone() children=move |c| {
                        let pick = c.value.clone();
                        view! {
                            <li><button type="button" class=c.tone on:click=move |_| {
                                open.set(false);
                                on_change.run(pick.clone());
                            }>{c.label}</button></li>
                        }
                    } />
                    <Show when=move || shown.with(Vec::is_empty)><li class="none">"No results"</li></Show>
                </ul>
            </div>
        </Show>
    }
}

/// A level out of `max`: a thin cell for none, then a cell for each step, lit from the left
#[component]
pub fn Hours(
    #[prop(into)] value: Signal<i64>,
    on_change: Callback<u8>,
    #[prop(default = 8)] max: u8,
) -> impl IntoView {
    view! {
        <div class="hours" role="group" aria-label="Level">
            <button
                type="button"
                class="cell none"
                class:lit=move || value.get() == 0
                title=format!("0/{max}")
                on:click=move |_| on_change.run(0)
            ></button>
            {(1..=max)
                .map(|n| {
                    let lit = move || value.get() >= i64::from(n);
                    view! {
                        <button
                            type="button"
                            class="cell"
                            class:lit=lit
                            title=format!("{n}/{max}")
                            on:click=move |_| on_change.run(n)
                        ></button>
                    }
                })
                .collect_view()}
            <span class="small">{move || format!("{}/{max}", value.get())}</span>
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
