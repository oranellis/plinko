//! The frame around every page: where you are, who you are, and how the connection is doing.

// modules {{{
use leptos::{prelude::*, task::spawn_local};
use plinko::multitenancy::user::Role;

use crate::{
    allocations::Allocations,
    api,
    availability::Availability as AvailabilityPage,
    finder::Finder,
    help::HelpButton,
    home::Home,
    icons::{Icon, IconKind},
    nav::Availability,
    nav::{Route, navigate, use_route},
    people::People,
    person::{PersonWindowView, provide_person_window},
    plan::PlanPage,
    plans::Plans,
    preferences::Preferences,
    ui::{Status, use_sync},
};
// }}}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Plans,
    AllPlans,
    Plan(plinko::common::short_id::ShortId),
    Allocations(uuid::Uuid),
    People,
    Preferences,
    Availability(crate::nav::Availability),
}

// Shell {{{
#[component]
pub fn Shell() -> impl IntoView {
    let sync = use_sync();
    let route = use_route();
    provide_context(route);
    // a person's window over the page goes when the page changes
    let person = provide_person_window();
    Effect::new(move |_| {
        route.track();
        person.set(None);
    });
    // changing tab or opening an item stays on the same page, so its state is kept
    let page = Memo::new(move |_| match route.get() {
        Route::Home => Page::Home,
        Route::Plans => Page::Plans,
        Route::AllPlans => Page::AllPlans,
        Route::Plan(plan, _, _) => Page::Plan(plan),
        Route::People => Page::People,
        Route::Allocations(user) => Page::Allocations(user),
        Route::Preferences => Page::Preferences,
        Route::Availability(of) => Page::Availability(of),
    });
    let name = move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.name().to_string()).unwrap_or_default())
    };
    let admin = move || sync.me.with(|m| m.as_ref().is_some_and(|m| m.admin));
    // everyone sees everyone, but only an admin changes the hours of other people and the
    // default ones, wherever the address says to go
    let allowed = Memo::new(move |_| {
        let me = sync.me.with(|m| m.as_ref().map(|m| m.id));
        let admins = match page.get() {
            Page::Availability(Availability::Default) => true,
            Page::Availability(Availability::User(user)) => Some(user) != me,
            _ => false,
        };
        !admins || admin()
    });
    let _ = Role::Admin;
    let logout = move |()| {
        spawn_local(async move {
            api::logout().await;
            sync.end();
        });
    };
    let link =
        move |label: &'static str, icon: IconKind, to: Signal<Route>, active: Signal<bool>| {
            view! {
                <a href=move || to.get().path() class:active=move || active.get()>
                    <Icon kind=icon size=16 />
                    {label}
                </a>
            }
        };
    let on_home = Signal::derive(move || route.get() == Route::Home);
    let on_plans = Signal::derive(move || {
        matches!(
            route.get(),
            Route::Plans | Route::AllPlans | Route::Plan(..)
        )
    });
    let (plans, people) = (Signal::stored(Route::Plans), Signal::stored(Route::People));
    // a person's allocations and hours are reached from the people page, so that stays lit
    let on_people = Signal::derive(move || {
        matches!(
            route.get(),
            Route::People
                | Route::Allocations(_)
                | Route::Availability(Availability::User(_) | Availability::Default)
        )
    });
    view! {
        <div class="shell">
            <header class="top">
                <a class="brand" href=Route::Home.path() title="Home">
                    <img src="plinko_logo.svg" alt="" />
                    <span class="word" class:away=move || !on_home.get()>"Plinko"</span>
                </a>
                <span class="tenant">{name}</span>
                <nav class="tabs" aria-label="Main">
                {link("Plans", IconKind::Gantt, plans, on_plans)}
                    {link("People", IconKind::Users, people, on_people)}
                </nav>
                <Status />
                <HelpButton />
                <UserMenu on_logout=Callback::new(logout) />
            </header>
            <Toast />
            <Finder />
            <PersonWindowView />
            <main class="page">
                {move || match page.get() {
                    _ if !allowed.get() => view! { <p>"Only admins can see this."</p> }.into_any(),
                    Page::Home => view! { <Home /> }.into_any(),
                    Page::Plans => view! { <Plans /> }.into_any(),
                    Page::AllPlans => view! { <crate::allplans::AllPlans /> }.into_any(),
                    Page::Plan(plan) => view! { <PlanPage plan=plan /> }.into_any(),
                    Page::Allocations(of) => view! { <Allocations of=of /> }.into_any(),
                    Page::People => view! { <People /> }.into_any(),
                    Page::Preferences => view! { <Preferences /> }.into_any(),
                    Page::Availability(of) => view! { <AvailabilityPage of=of /> }.into_any(),
                }}
            </main>
            <nav class="bottom" aria-label="Main">
                {link("Plans", IconKind::Gantt, plans, on_plans)}
                {link("People", IconKind::Users, people, on_people)}
            </nav>
        </div>
    }
}
// }}}

// UserMenu {{{
/// The person circle at the top right, and the menu it opens
#[component]
fn UserMenu(on_logout: Callback<()>) -> impl IntoView {
    let sync = use_sync();
    let open = RwSignal::new(false);
    crate::escape::on_escape(move || open.get(), Callback::new(move |()| open.set(false)));
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
// }}}

// account_names {{{
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
// }}}

// open_plan {{{
pub fn open_plan(plan: plinko::common::short_id::ShortId) {
    navigate(Route::Plan(plan, crate::nav::PlanTab::Gantt, None));
}
// }}}

// Toast {{{
/// How long the last thing that went wrong stays at the bottom of the screen
const TOAST_FOR: std::time::Duration = std::time::Duration::from_secs(5);

/// The last thing that went wrong, at the bottom of the screen until it fades or is clicked
#[component]
fn Toast() -> impl IntoView {
    let sync = use_sync();
    let shown = StoredValue::new(0u32);
    Effect::new(move |_| {
        if sync.notice.with(Option::is_none) {
            return;
        }
        let now = shown.get_value() + 1;
        shown.set_value(now);
        let fade = move || {
            if shown.try_get_value() == Some(now) {
                sync.notice.try_set(None);
            }
        };
        set_timeout(fade, TOAST_FOR);
    });
    move || {
        sync.notice.get().map(|said| {
            view! {
                <div class="toast" role="alert" title="Dismiss" on:click=move |_| sync.notice.set(None)>
                    {said}
                </div>
            }
        })
    }
}
// }}}
