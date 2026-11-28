//! The eye button's view options.

// modules {{{
use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{multitenancy::tenant::Tenant, scheduler::state::TenantVolatileState};
use uuid::Uuid;

use crate::{
    gantt::scene::Shown,
    icons::{Icon, IconKind},
};
// }}}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewOptions {
    pub mine: bool,
    pub split: bool,
}

// use_view {{{
/// Options for the page being built, which start afresh each time it is opened
pub fn use_view() -> (Signal<ViewOptions>, Callback<ViewOptions>) {
    let options = RwSignal::new(ViewOptions::default());
    (options.into(), Callback::new(move |o| options.set(o)))
}
// }}}

// people {{{
/// Who gets a band: the viewer first, then the others by name
pub fn people(
    tenant: &Tenant,
    plans: &[Shown],
    me: Option<Uuid>,
    everyone: bool,
) -> Vec<(Uuid, String)> {
    let users = tenant.users();
    let mut ids: Vec<Uuid> = if everyone {
        users.keys().copied().collect()
    } else {
        let mut ids: Vec<Uuid> = plans
            .iter()
            .flat_map(|s| s.plan.users().keys().copied())
            .collect();
        for state in plans.iter().filter_map(|s| s.state) {
            for task in state.tasks.values() {
                ids.extend(task.assignments.keys().filter_map(|a| match a {
                    plinko::planner::items::task::Assignee::User(id) => Some(*id),
                    _ => None,
                }));
            }
        }
        ids
    };
    ids.sort();
    ids.dedup();
    let mut people: Vec<(Uuid, String)> = ids
        .into_iter()
        .filter_map(|id| Some((id, users.get(&id)?.name().to_string())))
        .collect();
    people.sort_by(|a, b| {
        (Some(a.0) != me, a.1.to_lowercase(), a.0).cmp(&(Some(b.0) != me, b.1.to_lowercase(), b.0))
    });
    people
}

/// A band for each person when split, or one of your own when only your items show
pub fn bands(
    tenant: &Tenant,
    plans: &[Shown],
    me: Option<Uuid>,
    options: ViewOptions,
    everyone: bool,
) -> Option<Vec<(Uuid, String)>> {
    if options.split {
        Some(people(tenant, plans, me, everyone))
    } else if options.mine {
        Some(
            me.into_iter()
                .map(|id| (id, "My Items".to_string()))
                .collect(),
        )
    } else {
        None
    }
}

/// Booked hours count every plan, including ones the viewer cannot see
pub fn load_of<'a>(
    tenant: &'a Tenant,
    schedule: Option<&'a TenantVolatileState>,
) -> impl Fn(Uuid, NaiveDate, NaiveDate) -> Vec<(u32, u8)> + 'a {
    move |who, from, to| {
        let Some(user) = tenant.users().get(&who) else {
            return Vec::new();
        };
        let calendar = user.availability().calendar(from, to);
        let booked = schedule.and_then(|s| s.users.get(&who));
        from.iter_days()
            .take_while(|d| *d <= to)
            .map(|day| {
                (
                    booked.map_or(0, |b| b.booked_on(day)),
                    calendar.on(day).map_or(0, |h| h.get()),
                )
            })
            .collect()
    }
}
// }}}

// ViewButton {{{
#[component]
pub fn ViewButton(
    options: Signal<ViewOptions>,
    on_change: Callback<ViewOptions>,
    /// Whether the plan shows as a list, for a chart that has one
    #[prop(optional)]
    list: Option<(Signal<bool>, Callback<bool>)>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    crate::escape::on_escape(move || open.get(), Callback::new(move |()| open.set(false)));
    let toggle = move |label: &'static str,
                       get: fn(ViewOptions) -> bool,
                       set: fn(&mut ViewOptions, bool)| {
        view! {
            <button
                type="button"
                class="viewopt"
                class:on=move || get(options.get())
                aria-pressed=move || get(options.get()).to_string()
                on:click=move |_| {
                    let mut o = options.get_untracked();
                    let on = !get(o);
                    set(&mut o, on);
                    on_change.run(o);
                }
            >
                {label}
            </button>
        }
    };
    let filtered = move || {
        options.get() != ViewOptions::default() || list.is_some_and(|(listed, _)| listed.get())
    };
    view! {
        <div class="viewmenu keeps">
            <button
                type="button"
                class="toolbar-btn"
                class:active=move || open.get()
                class:lit=filtered
                title="View options"
                aria-label="View options"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <Icon kind=IconKind::Eye />
            </button>
            <Show when=move || open.get()>
                <div class="menu-backdrop" on:click=move |_| open.set(false)></div>
                <div class="viewpop">
                    {list.map(|(listed, set_listed)| view! {
                        <button
                            type="button"
                            class="viewopt"
                            class:on=move || listed.get()
                            aria-pressed=move || listed.get().to_string()
                            on:click=move |_| {
                                set_listed.run(!listed.get_untracked());
                                open.set(false);
                            }
                        >
                            "List view"
                        </button>
                        <hr class="viewsep" />
                    })}
                    {toggle("My items", |o| o.mine, |o, on| *o = ViewOptions { mine: on, split: o.split && !on })}
                    {toggle("Split by person", |o| o.split, |o, on| *o = ViewOptions { split: on, mine: o.mine && !on })}
                </div>
            </Show>
        </div>
    }
}
// }}}
