//! The search over the whole app, opened with control-k: the pages and the plans, to go to one.

// modules {{{
use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    icons::{Icon, IconKind},
    nav::{Availability, PlanTab, Route},
    ui::{Modal, use_sync},
};
// }}}

/// Somewhere the search goes to
#[derive(Clone, PartialEq)]
struct Place {
    name: String,
    icon: IconKind,
    to: Route,
}

// Finder {{{
/// Opens the search on control-k from anywhere, even a box being typed in, and closes it again
#[component]
pub fn Finder() -> impl IntoView {
    let open = RwSignal::new(false);
    let on_key =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            if (ev.ctrl_key() || ev.meta_key()) && ev.key().eq_ignore_ascii_case("k") {
                ev.prevent_default();
                open.try_update(|o| *o = !*o);
            }
        });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    }
    on_key.forget();
    view! {
        <Show when=move || open.get()>
            <FinderWindow on_close=Callback::new(move |()| open.set(false)) />
        </Show>
    }
}
// }}}

// FinderWindow {{{
/// The pages and plans that match what is typed, best first. Each is a link, so leaving an edit
/// with changes asks first, and enter clicks the one the arrows are on.
#[component]
fn FinderWindow(on_close: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let filter = RwSignal::new(String::new());
    let input = NodeRef::<leptos::html::Input>::new();
    let list = NodeRef::<leptos::html::Ul>::new();
    Effect::new(move |_| {
        if let Some(input) = input.get() {
            let _ = input.focus();
        }
    });
    let at = RwSignal::new(0usize);
    let places = Memo::new(move |_| {
        let me = sync.me.with(|m| m.as_ref().map(|m| (m.id, m.admin)));
        let (me, admin) = me.unwrap_or_default();
        let page = |name: &str, icon, to| Place {
            name: name.to_string(),
            icon,
            to,
        };
        let mut places = vec![
            page("Home", IconKind::Home, Route::Home),
            page("Plans", IconKind::Gantt, Route::Plans),
            page("Allocations", IconKind::Allocation, Route::Allocations),
            page(
                "Availability",
                IconKind::Resources,
                Route::Availability(Availability::User(me)),
            ),
        ];
        if admin {
            places.push(page("People", IconKind::Users, Route::People));
        }
        let mut plans: Vec<Place> = sync.tenant.with(|t| {
            let plans = t.as_ref().map(|t| t.plans().iter());
            plans
                .into_iter()
                .flatten()
                .map(|(id, p)| {
                    page(
                        p.name(),
                        IconKind::List,
                        Route::Plan(*id, PlanTab::Gantt, None),
                    )
                })
                .collect()
        });
        plans.sort_by(|a, b| a.name.cmp(&b.name).then(a.to.path().cmp(&b.to.path())));
        places.extend(plans);
        places
    });
    // the best matches first, pages before plans when they match as well
    let results = Memo::new(move |_| {
        let wanted = filter.get();
        let mut found: Vec<(i32, usize, Place)> = places.with(|p| {
            p.iter()
                .enumerate()
                .filter_map(|(i, p)| Some((crate::fuzzy::score(&wanted, &p.name)?, i, p.clone())))
                .collect()
        });
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        found.into_iter().map(|(_, _, p)| p).collect::<Vec<_>>()
    });
    let click_at = move || {
        let link = list
            .get_untracked()
            .and_then(|l| l.query_selector("a.at").ok().flatten())
            .and_then(|a| a.dyn_into::<web_sys::HtmlElement>().ok());
        if let Some(link) = link {
            link.click();
        }
    };
    view! {
        <Modal title="Go to" on_close=on_close>
            <input
                type="text"
                class="search"
                node_ref=input
                placeholder="Pages and plans…"
                prop:value=move || filter.get()
                class:nothing=move || !filter.with(String::is_empty) && results.with(Vec::is_empty)
                on:input=move |ev| {
                    at.set(0);
                    filter.set(event_target_value(&ev));
                }
                on:keydown=move |ev| {
                    let count = results.with(Vec::len);
                    match ev.key().as_str() {
                        "ArrowDown" if count > 0 => {
                            ev.prevent_default();
                            at.update(|a| *a = (*a + 1).min(count - 1));
                        }
                        "ArrowUp" => {
                            ev.prevent_default();
                            at.update(|a| *a = a.saturating_sub(1));
                        }
                        "Enter" => {
                            ev.prevent_default();
                            click_at();
                        }
                        _ => {}
                    }
                }
            />
            <ul class="results" node_ref=list>
                <Show when=move || results.with(Vec::is_empty)>
                    <li class="none">"No results"</li>
                </Show>
                <For each=move || results.get().into_iter().enumerate() key=|(i, p)| (*i, p.to.path(), p.name.clone()) children=move |(i, place)| {
                    let plan = matches!(place.to, Route::Plan(..));
                    view! {
                        <li>
                            <a
                                href=place.to.path()
                                class:at=move || at.get() == i
                                on:click=move |_| on_close.run(())
                                on:pointerenter=move |_| at.set(i)
                            >
                                <span class="glyph"><Icon kind=place.icon size=14 /></span>
                                {place.name}
                                {plan.then(|| view! { <span class="picked">"Plan"</span> })}
                            </a>
                        </li>
                    }
                } />
            </ul>
        </Modal>
    }
}
// }}}
