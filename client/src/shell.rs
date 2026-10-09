//! The frame around every page: where you are, who you are, and how the connection is doing.

use leptos::{prelude::*, task::spawn_local};
use plinko::multitenancy::user::Role;

use crate::{
    api,
    availability::Availability,
    home::Home,
    icons::{Icon, IconKind},
    nav::{Route, navigate, use_route},
    people::People,
    plan::PlanPage,
    plans::Plans,
    preferences::Preferences,
    resources::Resources,
    ui::{Status, use_sync},
    work::Work,
};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Plans,
    Plan(plinko::types::short_id::ShortId),
    Work,
    Resources,
    People,
    Preferences,
    Availability(crate::nav::Availability),
}

#[component]
pub fn Shell() -> impl IntoView {
    let sync = use_sync();
    let route = use_route();
    provide_context(route);
    // changing tab or opening an item stays on the same page, so its state is kept
    let page = Memo::new(move |_| match route.get() {
        Route::Home => Page::Home,
        Route::Plans => Page::Plans,
        Route::Plan(plan, _, _) => Page::Plan(plan),
        Route::Work => Page::Work,
        Route::Resources => Page::Resources,
        Route::People => Page::People,
        Route::Preferences => Page::Preferences,
        Route::Availability(of) => Page::Availability(of),
    });
    let name = move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.name().to_string()).unwrap_or_default())
    };
    let admin = move || sync.me.with(|m| m.as_ref().is_some_and(|m| m.admin));
    let _ = Role::Admin;
    let logout = move |()| {
        spawn_local(async move {
            api::logout().await;
            sync.me.set(None);
            sync.tenant.set(None);
        });
    };
    let link = move |label: &'static str, icon: IconKind, to: Route, active: Signal<bool>| {
        view! {
            <a href=to.path() class:active=move || active.get()>
                <Icon kind=icon size=16 />
                {label}
            </a>
        }
    };
    let on_home = Signal::derive(move || route.get() == Route::Home);
    let on_plans = Signal::derive(move || matches!(route.get(), Route::Plans | Route::Plan(..)));
    let on_work = Signal::derive(move || route.get() == Route::Work);
    let on_resources =
        Signal::derive(move || matches!(route.get(), Route::Resources | Route::Availability(_)));
    let on_people = Signal::derive(move || route.get() == Route::People);
    view! {
        <div class="shell">
            <header class="top">
                <a class="brand" href=Route::Home.path()>
                    <img src="plinko_logo.svg" alt="" />
                    "Plinko"
                </a>
                <span class="tenant">{name}</span>
                <nav class="tabs" aria-label="Main">
                    {link("Home", IconKind::Home, Route::Home, on_home)}
                {link("Plans", IconKind::Gantt, Route::Plans, on_plans)}
                    {link("My work", IconKind::Allocation, Route::Work, on_work)}
                    {link("Resources", IconKind::Resources, Route::Resources, on_resources)}
                    <Show when=admin>{link("People", IconKind::Users, Route::People, on_people)}</Show>
                </nav>
                <Status />
                <UserMenu on_logout=Callback::new(logout) />
            </header>
            <Show when=move || sync.notice.with(Option::is_some)>
                <div class="notice" role="alert">
                    {move || sync.notice.get()}
                    <button type="button" class="plain" on:click=move |_| sync.notice.set(None)>"Dismiss"</button>
                </div>
            </Show>
            <main class="page">
                {move || match page.get() {
                    Page::Home => view! { <Home /> }.into_any(),
                    Page::Plans => view! { <Plans /> }.into_any(),
                    Page::Plan(plan) => view! { <PlanPage plan=plan /> }.into_any(),
                    Page::Work => view! { <Work /> }.into_any(),
                    Page::Resources => view! { <Resources /> }.into_any(),
                    Page::People => view! { <People /> }.into_any(),
                    Page::Preferences => view! { <Preferences /> }.into_any(),
                    Page::Availability(of) => view! { <Availability of=of /> }.into_any(),
                }}
            </main>
            <nav class="bottom" aria-label="Main">
                {link("Home", IconKind::Home, Route::Home, on_home)}
                {link("Plans", IconKind::Gantt, Route::Plans, on_plans)}
                {link("My work", IconKind::Allocation, Route::Work, on_work)}
                {link("Resources", IconKind::Resources, Route::Resources, on_resources)}
                <Show when=admin>{link("People", IconKind::Users, Route::People, on_people)}</Show>
            </nav>
        </div>
    }
}

/// The person circle at the top right, and the menu it opens
#[component]
fn UserMenu(on_logout: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let open = RwSignal::new(false);
    let (name, email) = account_names(sync);
    view! {
        <div class="user-menu">
            <button
                type="button"
                class="plain avatar"
                title="Your account"
                aria-label="Your account"
                aria-haspopup="menu"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <Icon kind=IconKind::Person size=26 />
            </button>
            <Show when=move || open.get()>
                <div class="menu-backdrop" on:click=move |_| open.set(false)></div>
                <div class="menu" role="menu">
                    <div class="who">
                        <strong>{name}</strong>
                        <span class="small">{email}</span>
                    </div>
                    <a role="menuitem" href=Route::Preferences.path() on:click=move |_| open.set(false)>"Preferences"</a>
                    <button type="button" role="menuitem" on:click=move |_| { open.set(false); on_logout.run(()); }>
                        "Log out"
                    </button>
                </div>
            </Show>
        </div>
    }
}

/// The name and email of the person signed in, which follow their profile as it changes
fn account_names(sync: crate::sync::Sync) -> (Signal<String>, Signal<String>) {
    let me = move || sync.me.with(|m| m.as_ref().map(|m| m.id));
    let name = move || {
        let id = me()?;
        let known = sync.tenant.with(|t| {
            t.as_ref()
                .and_then(|t| t.users().get(&id))
                .map(|u| u.name().to_string())
        });
        known.or_else(|| sync.me.with(|m| m.as_ref().map(|m| m.name.clone())))
    };
    let email = move || {
        let id = me()?;
        sync.tenant.with(|t| {
            t.as_ref()
                .and_then(|t| t.users().get(&id))
                .map(|u| u.email().to_string())
        })
    };
    (
        Signal::derive(move || name().unwrap_or_default()),
        Signal::derive(move || email().unwrap_or_default()),
    )
}

pub fn open_plan(plan: plinko::types::short_id::ShortId) {
    navigate(Route::Plan(plan, crate::nav::PlanTab::Gantt, None));
}
