//! A person: who they are, what they are booked for over the next two weeks and when they work.
//! Shown at the top of the people page for whoever is signed in, and in a window over any page
//! for anyone, opened by clicking them wherever they are named.

// modules {{{
use chrono::{Datelike, Days, NaiveDate, Weekday};
use leptos::prelude::*;
use plinko::{
    availability::{rule::DEFAULT_RULE, rule_set::RuleSet, workload::Workload},
    common::crud::Crud,
    multitenancy::{tenant_update::TenantUpdateType, user::Role},
};
use uuid::Uuid;

use crate::{
    allocations::{HourPie, colour_of},
    format::{self, use_preferences},
    icons::IconKind,
    mine::{MyTask, use_tasks_of},
    nav::{Availability, PlanTab, Reveal, Route, navigate},
    people::{FormMode, PersonForm},
    ui::{IconButton, Modal, use_sync, use_today},
};
// }}}

// constants {{{
/// The days the person view looks ahead, today first
pub const AHEAD: u64 = 14;
/// The weeks of hours shown, from the start of this one
const HOUR_WEEKS: u64 = 4;
/// The most tasks listed before the rest are left to the full allocations
const MOST_TASKS: usize = 8;
/// Colours of the circles people are shown as, one each, the same every time
const FACES: [&str; 8] = [
    "#5b7fbf", "#4f9a78", "#b07a3c", "#9a5bb0", "#3f9aa6", "#b05b6e", "#7a8f3c", "#6b6bb8",
];
// }}}

// PersonWindow {{{
/// Who the window over the page is showing, if anyone
#[derive(Clone, Copy)]
pub struct PersonWindow(pub RwSignal<Option<Uuid>>);

/// Sets up the window for the pages under it to open
pub fn provide_person_window() -> RwSignal<Option<Uuid>> {
    let open = RwSignal::new(None);
    provide_context(PersonWindow(open));
    open
}

/// Opens a person's window, made while a page is being built so a click can use it later
pub fn use_open_person() -> Callback<Uuid> {
    let open = expect_context::<PersonWindow>().0;
    Callback::new(move |id| open.set(Some(id)))
}
// }}}

// helpers {{{
/// The first letters of the first two words of a name
pub fn initials(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() {
        "?".to_string()
    } else {
        letters
    }
}

fn face_of(id: Uuid) -> &'static str {
    FACES[(id.as_u128() % FACES.len() as u128) as usize]
}

/// A day of someone's: the hours they are booked for and the hours they have
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    pub day: NaiveDate,
    pub booked: u32,
    pub capacity: u8,
}

/// How booked someone is over `days` from today, with what is booked on plans you cannot see
pub fn use_load(who: Uuid, days: u64) -> Memo<Vec<Load>> {
    let sync = use_sync();
    let today = use_today();
    Memo::new(move |_| {
        let first = today.get();
        let last = first + Days::new(days.saturating_sub(1));
        sync.schedule.with(|schedule| {
            sync.tenant.with(|t| {
                let Some(user) = t.as_ref().and_then(|t| t.users().get(&who)) else {
                    return Vec::new();
                };
                let calendar = user.availability().calendar(first, last);
                let booked = schedule.as_ref().and_then(|(_, s)| s.users.get(&who));
                first
                    .iter_days()
                    .take_while(|d| *d <= last)
                    .map(|day| Load {
                        day,
                        booked: booked.map_or(0, |b| b.booked_on(day)),
                        capacity: calendar.on(day).map_or(0, |h| h.get()),
                    })
                    .collect()
            })
        })
    })
}

/// How booked the days are said in a word or two, and whether that is too much
pub fn busy(days: &[Load]) -> (String, &'static str) {
    let booked: u32 = days.iter().map(|d| d.booked).sum();
    let capacity: u32 = days.iter().map(|d| u32::from(d.capacity)).sum();
    if booked == 0 {
        return ("free".to_string(), "free");
    }
    if capacity == 0 {
        return ("booked with no hours".to_string(), "over");
    }
    let share = (f64::from(booked) / f64::from(capacity) * 100.0).round() as u32;
    let tone = match share {
        101.. => "over",
        85..=100 => "full",
        _ => "",
    };
    (format!("{share}% booked"), tone)
}

/// The run of days a task is on within the days shown, as grid columns from 1
fn span(task: &MyTask, first: NaiveDate, last: NaiveDate) -> Option<(i64, i64)> {
    if task.end < first || task.start > last {
        return None;
    }
    let from = (task.start.max(first) - first).num_days() + 1;
    let to = (task.end.min(last) - first).num_days() + 2;
    Some((from, to))
}

/// Runs of days with something on them, as grid columns from 1, for work that is not named
fn runs(hours: &[u32]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start = None;
    for (i, h) in hours.iter().chain(std::iter::once(&0)).enumerate() {
        match (start, *h > 0) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                runs.push((s + 1, i + 1));
                start = None;
            }
            _ => {}
        }
    }
    runs
}

/// Someone's hours on each day of the weeks shown, and whether a rule starts there
fn hour_weeks(set: &RuleSet<Workload>, from: NaiveDate) -> Vec<(NaiveDate, u8, bool)> {
    let to = from + Days::new(HOUR_WEEKS * 7 - 1);
    let calendar = set.calendar(from, to);
    let starts: std::collections::HashSet<NaiveDate> = set
        .rules()
        .values()
        .filter(|r| *r.id() != DEFAULT_RULE)
        .map(|r| r.start_day())
        .collect();
    from.iter_days()
        .take_while(|d| *d <= to)
        .map(|d| {
            (
                d,
                calendar.on(d).map_or(0, |h| h.get()),
                starts.contains(&d),
            )
        })
        .collect()
}
// }}}

// Face {{{
/// A person as a coloured circle with their initials
#[component]
pub fn Face(
    id: Uuid,
    #[prop(into)] name: Signal<String>,
    /// How wide, in pixels
    #[prop(default = 28)]
    size: u32,
) -> impl IntoView {
    view! {
        <span
            class="face"
            style=format!("background:{};--size:{size}px", face_of(id))
            aria-hidden="true"
        >
            {move || initials(&name.get())}
        </span>
    }
}
// }}}

// PersonButton {{{
/// A person's name that opens their window
#[component]
pub fn PersonButton(id: Uuid, #[prop(into)] name: String) -> impl IntoView {
    let open = use_open_person();
    let title = format!("About {name}");
    view! {
        <button
            type="button"
            class="person-link"
            title=title
            on:click=move |ev| {
                ev.stop_propagation();
                open.run(id);
            }
        >
            {name}
        </button>
    }
}
// }}}

// PersonView {{{
/// Who someone is, what they are booked for over the next two weeks and when they work, with
/// what an admin, or the person themselves, can change about them
#[component]
pub fn PersonView(
    who: Uuid,
    /// Set in a window, which it closes
    #[prop(optional)]
    on_close: Option<Callback<()>>,
) -> impl IntoView {
    let sync = use_sync();
    let me = move || sync.me.with(|m| m.as_ref().map(|m| (m.id, m.admin)));
    let yours = move || me().is_some_and(|(id, _)| id == who);
    let admin = move || me().is_some_and(|(_, admin)| admin);
    let can_edit = move || yours() || admin();
    let profile = Memo::new(move |_| {
        sync.tenant.with(|t| {
            let t = t.as_ref()?;
            let u = t.users().get(&who)?;
            let mut groups: Vec<String> = t
                .user_groups()
                .values()
                .filter(|g| g.users().contains(&who))
                .map(|g| g.name().to_string())
                .collect();
            groups.sort();
            Some((
                u.name().to_string(),
                u.email().to_string(),
                u.role() == Role::Admin,
                groups,
            ))
        })
    });
    let name = Signal::derive(move || {
        profile
            .with(|p| p.as_ref().map(|p| p.0.clone()))
            .unwrap_or_default()
    });
    let editing = RwSignal::new(None::<FormMode>);
    let removing = RwSignal::new(false);
    view! {
        <Show when=move || profile.with(Option::is_some) fallback=|| view! { <p class="small">"This person is no longer here."</p> }>
            <div class="person" class:inwindow=on_close.is_some()>
                <header class="person-head">
                    <Face id=who name=name size=48 />
                    <div class="person-names">
                        <div class="person-title">
                            <strong>{name}</strong>
                            {move || profile.with(|p| p.as_ref().is_some_and(|p| p.2)).then(|| view! { <span class="rolebadge">"admin"</span> })}
                        </div>
                        <span class="small">{move || profile.with(|p| p.as_ref().map(|p| p.1.clone()))}</span>
                        <div class="chips">
                            {move || profile.with(|p| p.as_ref().map(|p| p.3.clone())).unwrap_or_default().into_iter().map(|g| view! {
                                <span class="chip">{g}</span>
                            }).collect_view()}
                        </div>
                    </div>
                    {move || (yours() && on_close.is_none()).then(|| view! {
                        <a class="quiet" href=Route::Preferences.path()>"Preferences ›"</a>
                    })}
                    {on_close.map(|close| view! { <IconButton kind=IconKind::Close label="Close" on_click=close /> })}
                </header>
                <Booked who=who />
                <HoursStrip who=who editable=Signal::derive(can_edit) />
                <Show when=can_edit>
                    <footer class="person-actions">
                        <button type="button" on:click=move |_| editing.set(Some(FormMode::Details))>"Edit details"</button>
                        <Show when=move || admin() && !yours()>
                            <button type="button" on:click=move |_| editing.set(Some(FormMode::Password))>"Set password"</button>
                            <button
                                type="button"
                                class="danger"
                                on:click=move |_| {
                                    if !removing.get_untracked() {
                                        removing.set(true);
                                        return;
                                    }
                                    sync.send(TenantUpdateType::User(Crud::Remove(who)));
                                    if let Some(close) = on_close {
                                        close.run(());
                                    }
                                }
                                on:mouseleave=move |_| removing.set(false)
                            >
                                {move || if removing.get() { "Click again to remove" } else { "Remove" }}
                            </button>
                        </Show>
                    </footer>
                </Show>
            </div>
            {move || editing.get().map(|mode| view! {
                <PersonForm which=Some(who) mode=mode on_close=Callback::new(move |()| editing.set(None)) />
            })}
        </Show>
    }
}
// }}}

// Booked {{{
/// The next two weeks: a pie of each day's hours, and the tasks as bars under them
#[component]
fn Booked(who: Uuid) -> impl IntoView {
    let load = use_load(who, AHEAD);
    let tasks = use_tasks_of(Some(who));
    let today = use_today();
    let reveal = expect_context::<Reveal>().0;
    let shown = Memo::new(move |_| {
        let first = today.get();
        let last = first + Days::new(AHEAD - 1);
        tasks.with(|t| {
            t.iter()
                .filter_map(|t| Some((t.clone(), span(t, first, last)?)))
                .collect::<Vec<_>>()
        })
    });
    // what is booked on plans that cannot be seen from here: hours with no task shown for them
    let other = Memo::new(move |_| {
        let named = |day: NaiveDate| -> u32 {
            tasks.with(|t| {
                t.iter()
                    .map(|t| u32::from(t.days.get(&day).copied().unwrap_or(0)))
                    .sum()
            })
        };
        load.with(|l| {
            l.iter()
                .map(|d| d.booked.saturating_sub(named(d.day)))
                .collect::<Vec<_>>()
        })
    });
    let said = Memo::new(move |_| load.with(|l| busy(l)));
    let nothing = move || shown.with(Vec::is_empty) && other.with(|o| o.iter().all(|h| *h == 0));
    view! {
        <section class="person-part">
            <div class="person-part-head">
                <h3>"Next two weeks"</h3>
                <span class=move || format!("busy {}", said.get().1)>{move || said.get().0}</span>
            </div>
            // a row each, every one the same columns: the names, then a column a day
            <div class="booked" style=format!("--days:{AHEAD}") class:none=nothing>
                <div class="brow">
                    {move || load.get().into_iter().enumerate().map(|(i, d)| {
                        let weekend = matches!(d.day.weekday(), Weekday::Sat | Weekday::Sun);
                        view! {
                            <span class="bday" class:off=weekend class:today=i == 0 style=format!("grid-column:{}", i + 2) title=format::day(d.day)>
                                {format::weekday(d.day).chars().next().unwrap_or(' ').to_string()}
                            </span>
                        }
                    }).collect_view()}
                </div>
                <div class="brow pies">
                    {move || load.get().into_iter().enumerate().map(|(i, d)| view! {
                        <span class="bpie" style=format!("grid-column:{}", i + 2)>
                            {(d.booked > 0).then(|| view! {
                                <HourPie booked=d.booked capacity=d.capacity title=format!("{}: {} of {} hours booked", format::day(d.day), d.booked, d.capacity) />
                            })}
                        </span>
                    }).collect_view()}
                </div>
                {move || {
                    let rows = shown.get();
                    let hidden = rows.len().saturating_sub(MOST_TASKS);
                    let bars = rows.into_iter().take(MOST_TASKS).map(|(task, (from, to))| {
                        let (plan, id) = (task.plan, task.task);
                        let go = move |_| {
                            reveal.set(Some((plan, id)));
                            navigate(Route::Plan(plan, PlanTab::Gantt, None));
                        };
                        view! {
                            <div class="brow">
                                <button type="button" class="blabel" title=format!("{} · {}", task.name, task.plan_name) on:click=go>
                                    {task.name.clone()}
                                </button>
                                <button
                                    type="button"
                                    class="bbar"
                                    aria-label=task.name.clone()
                                    title=format!("{} · {}\n{} to {}", task.name, task.plan_name, format::day(task.start), format::day(task.end))
                                    style=format!("grid-column:{} / {};background:{}", from + 1, to + 1, colour_of(id))
                                    on:click=go
                                ></button>
                            </div>
                        }
                    }).collect_view();
                    let more = (hidden > 0).then(|| view! {
                        <div class="brow"><span class="blabel more">{format!("and {hidden} more…")}</span></div>
                    });
                    (bars, more)
                }}
                {move || {
                    let runs = runs(&other.get());
                    (!runs.is_empty()).then(|| view! {
                        <div class="brow">
                            <span class="blabel ghost" title="Work on plans you are not in">"Other work"</span>
                            {runs.into_iter().map(|(from, to)| view! {
                                <span class="bbar ghost" style=format!("grid-column:{} / {}", from + 1, to + 1)></span>
                            }).collect_view()}
                        </div>
                    })
                }}
            </div>
            <Show when=nothing>
                <p class="small">"Nothing booked in the next two weeks."</p>
            </Show>
            <a class="quiet more-link" href=Route::Allocations(who).path()>"Full allocations ›"</a>
        </section>
    }
}
// }}}

// HoursStrip {{{
/// A few weeks of someone's hours, each day shaded by how many, a dot where a rule starts
#[component]
fn HoursStrip(who: Uuid, editable: Signal<bool>) -> impl IntoView {
    let sync = use_sync();
    let today = use_today();
    let week_start = use_preferences().week_start;
    let from = Memo::new(move |_| {
        let today = today.get();
        today - Days::new(week_start.get().offset(today) as u64)
    });
    let days = Memo::new(move |_| {
        let from = from.get();
        sync.tenant.with(|t| {
            t.as_ref()
                .and_then(|t| t.users().get(&who))
                .map(|u| hour_weeks(u.availability(), from))
                .unwrap_or_default()
        })
    });
    view! {
        <section class="person-part">
            <div class="person-part-head">
                <h3>"Hours"</h3>
                <span class="busy">{move || {
                    let week: u32 = days.with(|d| d.iter().take(7).map(|(_, h, _)| u32::from(*h)).sum());
                    format!("{week} h this week")
                }}</span>
            </div>
            <HourCells days=days today=today />
            <Show when=move || editable.get()>
                <a class="quiet more-link" href=Route::Availability(Availability::User(who)).path()>"Edit hours ›"</a>
            </Show>
        </section>
    }
}

/// Weeks of days in rows, shaded by their hours
#[component]
pub fn HourCells(
    #[prop(into)] days: Signal<Vec<(NaiveDate, u8, bool)>>,
    today: Memo<NaiveDate>,
) -> impl IntoView {
    let week_start = use_preferences().week_start;
    view! {
        <div class="hourcells">
            {move || week_start.get().weekdays().into_iter().map(|w| view! {
                <span class="hhead">{w.chars().next().unwrap_or(' ').to_string()}</span>
            }).collect_view()}
            {move || days.get().into_iter().map(|(d, hours, rule)| view! {
                <span
                    class="hcell"
                    class:today=d == today.get()
                    class:marked=rule
                    style=format!("--shade:{}", f64::from(hours) / 8.0)
                    title=format!("{}: {hours} h", format::day(d))
                ></span>
            }).collect_view()}
        </div>
    }
}

/// The hours of the weeks shown from a set of rules, for the default hours
pub fn hours_from(set: &RuleSet<Workload>, from: NaiveDate) -> Vec<(NaiveDate, u8, bool)> {
    hour_weeks(set, from)
}
// }}}

// PersonWindowView {{{
/// The window over the page for whoever was clicked, which goes when the page changes
#[component]
pub fn PersonWindowView() -> impl IntoView {
    let open = expect_context::<PersonWindow>().0;
    let close = Callback::new(move |()| open.set(None));
    let sync = use_sync();
    move || {
        open.get().map(|who| {
            let title = Signal::derive(move || {
                sync.tenant
                    .with(|t| {
                        t.as_ref()
                            .and_then(|t| t.users().get(&who))
                            .map(|u| u.name().to_string())
                    })
                    .unwrap_or_default()
            });
            view! {
                <Modal title=title on_close=close bare=true>
                    <PersonView who=who on_close=close />
                </Modal>
            }
        })
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 3, day).unwrap()
    }

    fn load(booked: u32, capacity: u8) -> Load {
        Load {
            day: d(1),
            booked,
            capacity,
        }
    }

    #[test]
    fn initials_are_the_first_two_words() {
        assert_eq!(initials("ana brook"), "AB");
        assert_eq!(initials("Jo"), "J");
        assert_eq!(initials("Mary Ann Smith"), "MA");
        assert_eq!(initials("  "), "?");
    }

    #[test]
    fn how_booked_is_said() {
        assert_eq!(busy(&[load(0, 8)]).0, "free");
        assert_eq!(
            busy(&[load(4, 8), load(4, 8)]),
            ("50% booked".to_string(), "")
        );
        assert_eq!(busy(&[load(8, 8)]).1, "full");
        assert_eq!(busy(&[load(9, 8)]).1, "over");
        assert_eq!(busy(&[load(2, 0)]).1, "over");
    }

    #[test]
    fn runs_are_the_days_with_something_on() {
        assert_eq!(runs(&[0, 2, 3, 0, 0, 1]), vec![(2, 4), (6, 7)]);
        assert_eq!(runs(&[0, 0]), vec![]);
        assert_eq!(runs(&[1]), vec![(1, 2)]);
    }

    #[test]
    fn a_task_spans_the_days_it_has_shown() {
        let task = |start, end| MyTask {
            plan: Default::default(),
            plan_name: String::new(),
            task: Uuid::nil(),
            name: String::new(),
            status: plinko::planner::items::task::Status::NotStarted,
            start: d(start),
            end: d(end),
            days: Default::default(),
        };
        assert_eq!(span(&task(1, 3), d(2), d(10)), Some((1, 3)));
        assert_eq!(span(&task(4, 20), d(2), d(10)), Some((3, 10)));
        assert_eq!(span(&task(11, 20), d(2), d(10)), None);
    }
}
// }}}
