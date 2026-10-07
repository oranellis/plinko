//! The frame around every page: where you are, who you are, and how the connection is doing.

use leptos::{prelude::*, task::spawn_local};
use plinko::multitenancy::user::Role;

use crate::{
    api,
    availability::Availability,
    nav::{Route, navigate, use_route},
    people::People,
    plan::PlanPage,
    plans::Plans,
    resources::Resources,
    ui::{Status, use_sync},
    work::Work,
};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Plans,
    Plan(plinko::types::short_id::ShortId),
    Work,
    Resources,
    People,
    Availability(crate::nav::Availability),
}

#[component]
pub fn Shell() -> impl IntoView {
    let sync = use_sync();
    let route = use_route();
    provide_context(route);
    // changing tab or opening an item stays on the same page, so its state is kept
    let page = Memo::new(move |_| match route.get() {
        Route::Plans => Page::Plans,
        Route::Plan(plan, _, _) => Page::Plan(plan),
        Route::Work => Page::Work,
        Route::Resources => Page::Resources,
        Route::People => Page::People,
        Route::Availability(of) => Page::Availability(of),
    });
    let name = move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.name().to_string()).unwrap_or_default())
    };
    let admin = move || sync.me.with(|m| m.as_ref().is_some_and(|m| m.admin));
    let _ = Role::Admin;
    let logout = move |_| {
        spawn_local(async move {
            api::logout().await;
            sync.me.set(None);
            sync.tenant.set(None);
        });
    };
    let link = move |label: &'static str, to: Route, active: Signal<bool>| {
        view! {
            <a href=to.path() class:active=move || active.get()>{label}</a>
        }
    };
    let on_plans = Signal::derive(move || matches!(route.get(), Route::Plans | Route::Plan(..)));
    let on_work = Signal::derive(move || route.get() == Route::Work);
    let on_resources =
        Signal::derive(move || matches!(route.get(), Route::Resources | Route::Availability(_)));
    let on_people = Signal::derive(move || route.get() == Route::People);
    view! {
        <div class="shell">
            <header class="top">
                <a class="brand" href=Route::Plans.path()>"Plinko"</a>
                <span class="tenant">{name}</span>
                <nav class="tabs" aria-label="Main">
                    {link("Plans", Route::Plans, on_plans)}
                    {link("My work", Route::Work, on_work)}
                    {link("Resources", Route::Resources, on_resources)}
                    <Show when=admin>{link("People", Route::People, on_people)}</Show>
                </nav>
                <Status />
                <button class="plain" on:click=logout>
                    {move || sync.me.with(|m| m.as_ref().map(|m| m.name.clone()).unwrap_or_default())}
                    " · Log out"
                </button>
            </header>
            <Show when=move || sync.notice.with(Option::is_some)>
                <div class="notice" role="alert">
                    {move || sync.notice.get()}
                    <button class="plain" on:click=move |_| sync.notice.set(None)>"Dismiss"</button>
                </div>
            </Show>
            <main class="page">
                {move || match page.get() {
                    Page::Plans => view! { <Plans /> }.into_any(),
                    Page::Plan(plan) => view! { <PlanPage plan=plan /> }.into_any(),
                    Page::Work => view! { <Work /> }.into_any(),
                    Page::Resources => view! { <Resources /> }.into_any(),
                    Page::People => view! { <People /> }.into_any(),
                    Page::Availability(of) => view! { <Availability of=of /> }.into_any(),
                }}
            </main>
            <nav class="bottom" aria-label="Main">
                {link("Plans", Route::Plans, on_plans)}
                {link("My work", Route::Work, on_work)}
                {link("Resources", Route::Resources, on_resources)}
                <Show when=admin>{link("People", Route::People, on_people)}</Show>
            </nav>
        </div>
    }
}

pub fn open_plan(plan: plinko::types::short_id::ShortId) {
    navigate(Route::Plan(plan, crate::nav::PlanTab::Gantt, None));
}
