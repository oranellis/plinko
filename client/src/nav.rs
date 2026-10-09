//! Where the user is, kept in the address (`#/plan/AbCdEfGh/gantt`) so links and the back button
//! work.

use leptos::prelude::*;
use plinko::types::short_id::ShortId;
use uuid::Uuid;
use wasm_bindgen::{JsCast, closure::Closure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanTab {
    Gantt,
    List,
    Settings,
}

impl PlanTab {
    fn name(self) -> &'static str {
        match self {
            PlanTab::Gantt => "gantt",
            PlanTab::List => "list",
            PlanTab::Settings => "settings",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [PlanTab::Gantt, PlanTab::List, PlanTab::Settings]
            .into_iter()
            .find(|t| t.name() == text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Home,
    Plans,
    /// A plan, and the item open in its editor
    Plan(ShortId, PlanTab, Option<Uuid>),
    Allocations,
    Resources,
    People,
    Preferences,
    /// Availability of a user, or the plan default of a plan
    Availability(Availability),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    User(Uuid),
    Plan(ShortId),
}

impl Route {
    pub fn path(&self) -> String {
        match self {
            Route::Home => "#/".to_string(),
            Route::Plans => "#/plans".to_string(),
            Route::Plan(plan, tab, open) => {
                let open = open.map(|id| format!("/{id}")).unwrap_or_default();
                format!("#/plan/{plan}/{}{open}", tab.name())
            }
            Route::Allocations => "#/allocations".to_string(),
            Route::Resources => "#/resources".to_string(),
            Route::People => "#/people".to_string(),
            Route::Preferences => "#/preferences".to_string(),
            Route::Availability(Availability::User(user)) => format!("#/availability/user/{user}"),
            Route::Availability(Availability::Plan(plan)) => format!("#/availability/plan/{plan}"),
        }
    }

    pub fn parse(hash: &str) -> Self {
        let path = hash.trim_start_matches('#').trim_matches('/');
        let parts: Vec<&str> = path.split('/').collect();
        match parts.as_slice() {
            ["plan", plan, rest @ ..] => {
                let Ok(plan) = ShortId::from_str_checked(plan) else {
                    return Route::Home;
                };
                let tab = rest.first().and_then(|t| PlanTab::parse(t));
                let open = rest.get(1).and_then(|id| Uuid::parse_str(id).ok());
                Route::Plan(plan, tab.unwrap_or(PlanTab::Gantt), open)
            }
            ["plans"] => Route::Plans,
            ["allocations"] => Route::Allocations,
            ["resources"] => Route::Resources,
            ["people"] => Route::People,
            ["preferences"] => Route::Preferences,
            ["availability", "user", user] => match Uuid::parse_str(user) {
                Ok(user) => Route::Availability(Availability::User(user)),
                Err(_) => Route::Home,
            },
            ["availability", "plan", plan] => match ShortId::from_str_checked(plan) {
                Ok(plan) => Route::Availability(Availability::Plan(plan)),
                Err(_) => Route::Home,
            },
            _ => Route::Home,
        }
    }
}

pub fn navigate(route: Route) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_hash(&route.path());
    }
}

/// The current route, following the address
pub fn use_route() -> RwSignal<Route> {
    let read = || {
        let hash = web_sys::window().and_then(|w| w.location().hash().ok());
        Route::parse(&hash.unwrap_or_default())
    };
    let route = RwSignal::new(read());
    let listener = Closure::<dyn FnMut()>::new(move || route.set(read()));
    if let Some(window) = web_sys::window() {
        let _ = window
            .add_event_listener_with_callback("hashchange", listener.as_ref().unchecked_ref());
    }
    listener.forget();
    route
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_survive_the_address() {
        let plan = ShortId::from_str_checked("AbCdEfGh").unwrap();
        let item = Uuid::from_u128(7);
        for route in [
            Route::Home,
            Route::Plans,
            Route::Plan(plan, PlanTab::List, None),
            Route::Plan(plan, PlanTab::Gantt, Some(item)),
            Route::Allocations,
            Route::Resources,
            Route::People,
            Route::Preferences,
            Route::Availability(Availability::User(item)),
            Route::Availability(Availability::Plan(plan)),
        ] {
            assert_eq!(Route::parse(&route.path()), route);
        }
    }

    #[test]
    fn nonsense_goes_home() {
        assert_eq!(Route::parse(""), Route::Home);
        assert_eq!(Route::parse("#/plan/short"), Route::Home);
        assert_eq!(Route::parse("#/nowhere/at/all"), Route::Home);
    }
}
