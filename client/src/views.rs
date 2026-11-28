//! The eye button's view options, kept in this browser for each plan and for all plans.

// modules {{{
use std::collections::HashMap;

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{multitenancy::tenant::Tenant, scheduler::state::TenantVolatileState};
use uuid::Uuid;

use crate::{
    gantt::scene::Shown,
    icons::{Icon, IconKind},
};
// }}}

const STORAGE: &str = "plinko.views";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewOptions {
    pub mine: bool,
    pub split: bool,
}

#[derive(Clone, Copy)]
struct Views(RwSignal<HashMap<String, ViewOptions>>);

// storage {{{
fn parse(saved: &str) -> HashMap<String, ViewOptions> {
    saved
        .split(';')
        .filter_map(|pair| {
            let (key, flags) = pair.split_once('=')?;
            let options = ViewOptions {
                mine: flags.contains('m'),
                split: flags.contains('s'),
            };
            Some((key.to_string(), options))
        })
        .collect()
}

fn write(views: &HashMap<String, ViewOptions>) -> String {
    let mut pairs: Vec<String> = views
        .iter()
        .filter(|(_, o)| **o != ViewOptions::default())
        .map(|(key, o)| {
            let flags = [(o.mine, "m"), (o.split, "s")]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, f)| *f)
                .collect::<String>();
            format!("{key}={flags}")
        })
        .collect();
    pairs.sort();
    pairs.join(";")
}
// }}}

// provide_views {{{
pub fn provide_views() {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    let saved = storage
        .as_ref()
        .and_then(|s| s.get_item(STORAGE).ok().flatten())
        .unwrap_or_default();
    let views = RwSignal::new(parse(&saved));
    Effect::new(move |_| {
        if let Some(s) = &storage {
            let _ = s.set_item(STORAGE, &views.with(write));
        }
    });
    provide_context(Views(views));
}

/// The options of one chart, `key` being `plan:<id>` or `all`
pub fn use_view(key: String) -> (Signal<ViewOptions>, Callback<ViewOptions>) {
    let views = expect_context::<Views>().0;
    let read_key = key.clone();
    let options =
        Signal::derive(move || views.with(|v| v.get(&read_key).copied().unwrap_or_default()));
    let set = Callback::new(move |o: ViewOptions| {
        views.update(|v| {
            v.insert(key.clone(), o);
        })
    });
    (options, set)
}
// }}}

// people {{{
/// Who gets a band: you first, then the others by name. With only your items that is just you.
pub fn people(
    tenant: &Tenant,
    plans: &[Shown],
    me: Option<Uuid>,
    everyone: bool,
    mine: bool,
) -> Vec<(Uuid, String)> {
    let users = tenant.users();
    let mut ids: Vec<Uuid> = if mine {
        me.into_iter().collect()
    } else if everyone {
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
pub fn ViewButton(options: Signal<ViewOptions>, on_change: Callback<ViewOptions>) -> impl IntoView {
    let open = RwSignal::new(false);
    crate::escape::on_escape(move || open.get(), Callback::new(move |()| open.set(false)));
    let toggle = move |label: &'static str,
                       get: fn(ViewOptions) -> bool,
                       set: fn(&mut ViewOptions, bool)| {
        view! {
            <label class="viewopt">
                <input
                    type="checkbox"
                    prop:checked=move || get(options.get())
                    on:change=move |ev| {
                        let mut o = options.get_untracked();
                        set(&mut o, event_target_checked(&ev));
                        on_change.run(o);
                    }
                />
                <span>{label}</span>
            </label>
        }
    };
    let filtered = move || options.get() != ViewOptions::default();
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
                    {toggle("My items", |o| o.mine, |o, on| o.mine = on)}
                    {toggle("Split by person", |o| o.split, |o, on| o.split = on)}
                </div>
            </Show>
        </div>
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_kept_as_text_and_read_back() {
        let mut views = HashMap::new();
        views.insert(
            "plan:abc".to_string(),
            ViewOptions {
                mine: true,
                split: true,
            },
        );
        views.insert(
            "all".to_string(),
            ViewOptions {
                mine: false,
                split: true,
            },
        );
        views.insert("plan:off".to_string(), ViewOptions::default());
        let text = write(&views);
        assert_eq!(text, "all=s;plan:abc=ms");
        views.remove("plan:off");
        assert_eq!(parse(&text), views);
    }
}
// }}}
