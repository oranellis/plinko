//! Routes, kept in the URL hash (`#/plan/AbCdEfGh/gantt`) so links and the back button work.

// modules {{{
use leptos::prelude::*;
use plinko::common::short_id::ShortId;
use uuid::Uuid;
// }}}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanTab {
    Gantt,
    List,
    Settings,
}

// impl PlanTab {{{
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
// }}}

// PlanTab supporting types {{{
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Home,
    Plans,
    AllPlans,
    /// The `Uuid` is the item open in the editor
    Plan(ShortId, PlanTab, Option<Uuid>),
    People,
    Allocations(Uuid),
    Preferences,
    Availability(Availability),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    User(Uuid),
    Plan(ShortId),
    /// What new users and new plans start with
    Default,
}

/// An item to show once its plan's chart opens, set from another page
#[derive(Clone, Copy)]
pub struct Reveal(pub RwSignal<Option<(ShortId, Uuid)>>);
// }}}

// impl Route {{{
impl Route {
    pub fn path(&self) -> String {
        match self {
            Route::Home => "#/".to_string(),
            Route::Plans => "#/plans".to_string(),
            Route::AllPlans => "#/plans/all".to_string(),
            Route::Plan(plan, tab, open) => {
                let open = open.map(|id| format!("/{id}")).unwrap_or_default();
                format!("#/plan/{plan}/{}{open}", tab.name())
            }
            Route::People => "#/people".to_string(),
            Route::Allocations(user) => format!("#/people/{user}/allocations"),
            Route::Preferences => "#/preferences".to_string(),
            Route::Availability(Availability::User(user)) => format!("#/people/{user}/hours"),
            Route::Availability(Availability::Plan(plan)) => format!("#/availability/plan/{plan}"),
            Route::Availability(Availability::Default) => "#/people/new-starters/hours".to_string(),
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
            ["plans", "all"] => Route::AllPlans,
            ["people"] => Route::People,
            ["people", "new-starters", "hours"] => Route::Availability(Availability::Default),
            ["people", user, "allocations"] => {
                Uuid::parse_str(user).map_or(Route::Home, Route::Allocations)
            }
            ["people", user, "hours"] => match Uuid::parse_str(user) {
                Ok(user) => Route::Availability(Availability::User(user)),
                Err(_) => Route::Home,
            },
            ["preferences"] => Route::Preferences,
            ["availability", "plan", plan] => match ShortId::from_str_checked(plan) {
                Ok(plan) => Route::Availability(Availability::Plan(plan)),
                Err(_) => Route::Home,
            },
            _ => Route::Home,
        }
    }
}
// }}}

// helpers {{{
pub fn navigate(route: Route) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_hash(&route.path());
    }
}

pub fn use_route() -> RwSignal<Route> {
    let read = || {
        let hash = web_sys::window().and_then(|w| w.location().hash().ok());
        Route::parse(&hash.unwrap_or_default())
    };
    let route = RwSignal::new(read());
    crate::ui::window_listener("hashchange", move |_: web_sys::Event| route.set(read()));
    route
}
// }}}

// tests {{{
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
            Route::AllPlans,
            Route::Plan(plan, PlanTab::List, None),
            Route::Plan(plan, PlanTab::Gantt, Some(item)),
            Route::People,
            Route::Allocations(item),
            Route::Preferences,
            Route::Availability(Availability::User(item)),
            Route::Availability(Availability::Plan(plan)),
            Route::Availability(Availability::Default),
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
// }}}
