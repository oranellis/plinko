//! Every task you are down for, across all plans, in the order they start. Your tasks are down the
//! left and a row of days runs across, each task showing the hours it takes you that day, and the
//! row at the top showing the hours you have each day and how many of them are booked.

use chrono::{Datelike, Days, NaiveDate, Weekday};
use leptos::{html, prelude::*};
use plinko::{
    scheduler::state::PlanVolatileState,
    types::{dependency::PlanNode, short_id::ShortId},
};
use uuid::Uuid;

use crate::{
    format,
    gantt::{arrange::Mark, info::info},
    guard::{ConfirmLeave, Guard},
    mine::{MyTask, use_my_tasks},
    plan::{ItemPane, blank_click},
    ui::use_sync,
};

/// How wide a day is and the column of names, which scrolling is worked out from
const DAY_WIDTH: f64 = 44.0;
const NAME_WIDTH: f64 = 240.0;
/// The weeks shown from before the current one
const BEFORE: u64 = 2;
/// The fewest weeks shown, and how many more than the scroll has reached
const LEAST: u64 = 12;
const AHEAD: u64 = 4;
/// The hours a day holds, which the boxes show
const FULL_DAY: u8 = 8;

const COLOURS: [&str; 10] = [
    "#4a90d9", "#7ed321", "#f5a623", "#d0021b", "#9b59b6", "#1abc9c", "#e67e22", "#2ecc71",
    "#e74c3c", "#3498db",
];

/// The colour a task is drawn in, the same every time
fn colour_of(task: Uuid) -> &'static str {
    COLOURS[(task.as_u128() % COLOURS.len() as u128) as usize]
}

/// Eight boxes, two wide and four tall, which the hours fill a column at a time from the bottom, with
/// the hours written over them. The boxes go from grey to green as the day fills.
#[component]
fn HourBoxes(hours: u32, #[prop(into)] title: String) -> impl IntoView {
    let filled = hours.min(u32::from(FULL_DAY));
    let green = filled * 100 / u32::from(FULL_DAY);
    view! {
        <div class="hourboxes" title=title>
            <div class="boxes">
                {(0..FULL_DAY)
                    .map(|k| {
                        let on = u32::from(k) < filled;
                        let colour = if on {
                            format!("color-mix(in srgb, #7c7c80, #43c463 {green}%)")
                        } else {
                            "#2c2c2e".to_string()
                        };
                        // columns fill from the bottom up, left to right
                        let place = format!(
                            "grid-row:{};grid-column:{};background:{colour}",
                            4 - k % 4,
                            1 + k / 4,
                        );
                        view! { <i style=place></i> }
                    })
                    .collect_view()}
            </div>
            <span class="num">{hours}</span>
        </div>
    }
}

#[component]
pub fn Allocations() -> impl IntoView {
    let sync = use_sync();
    let mine = use_my_tasks();
    let today = Memo::new(move |_| {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.today()).unwrap_or_default())
    });
    let scroller = NodeRef::<html::Div>::new();
    // weeks shown, which grow as the table is scrolled forward and shrink as it comes back
    let weeks = RwSignal::new(LEAST);
    let first = Memo::new(move |_| {
        let today = today.get();
        let earliest = mine.with(|m| m.iter().map(|t| t.start).min());
        let from = earliest.map_or(today, |e| e.min(today));
        let monday = from - Days::new(u64::from(from.weekday().num_days_from_monday()));
        monday - Days::new(BEFORE * 7)
    });
    let measure = move || {
        if let Some(element) = scroller.get_untracked() {
            let reached = f64::from(element.scroll_left() + element.client_width());
            let days = ((reached - NAME_WIDTH) / DAY_WIDTH).max(0.0) as u64;
            weeks.set((days / 7 + 1 + AHEAD).max(LEAST));
        }
    };
    let to_today = move || {
        if let Some(element) = scroller.get_untracked() {
            let at = (today.get_untracked() - first.get_untracked()).num_days() as f64 * DAY_WIDTH;
            // today a little in from the names
            element.set_scroll_left((at - 2.0 * DAY_WIDTH).max(0.0) as i32);
            measure();
        }
    };
    Effect::new(move |_| {
        if scroller.get().is_some() {
            to_today();
        }
    });
    let days = Memo::new(move |_| {
        let first = first.get();
        (0..weeks.get() * 7)
            .map(|d| first + Days::new(d))
            .collect::<Vec<NaiveDate>>()
    });
    // the hours the person has and has booked each day shown
    let hours = Memo::new(move |_| {
        let me = sync.me.with(|m| m.as_ref().map(|m| m.id))?;
        let days = days.get();
        let (first, last) = (*days.first()?, *days.last()?);
        let schedule = sync.schedule.get();
        sync.tenant.with(|t| {
            let user = t.as_ref()?.users().get(&me)?;
            let calendar = user.availability().calendar(first, last);
            let booked = schedule.as_ref().and_then(|(_, s)| s.users.get(&me));
            Some(
                days.iter()
                    .map(|d| {
                        (
                            booked.map_or(0, |b| b.booked_on(*d)),
                            calendar.on(*d).map_or(0, |h| h.get()),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
    });
    // what is said of a task when the pointer rests on it, as in the chart
    let said = move |task: &MyTask| {
        let state: Option<PlanVolatileState> = sync.schedule.with(|s| {
            s.as_ref()
                .and_then(|(_, s)| s.plans.get(&task.plan).cloned())
        });
        let lines = sync
            .tenant
            .with(|t| {
                let t = t.as_ref()?;
                Some(info(
                    t,
                    t.plans().get(&task.plan)?,
                    state.as_ref(),
                    Mark::Node(PlanNode::Task(task.task)),
                ))
            })
            .unwrap_or_default();
        let mut lines = lines;
        lines.insert(1.min(lines.len()), format!("Plan: {}", task.plan_name));
        lines.join("\n")
    };
    let open = RwSignal::new(None::<(ShortId, PlanNode)>);
    let fresh = RwSignal::new(None::<Uuid>);
    let guard = Guard::new();
    let close = Callback::new(move |()| open.set(None));
    let blank = move || {
        if open.get_untracked().is_some() {
            guard.then(close);
        }
    };
    view! {
        <div class="plan">
            <ConfirmLeave guard=guard />
            <div class="bar allocbar">
                <h1>"Allocations"</h1>
                <button type="button" on:click=move |_| to_today()>"Today"</button>
            </div>
            <div class="workspace">
                <div class="main" on:click=move |ev| if blank_click(&ev) { blank() }>
                    <Show
                        when=move || mine.with(|m| !m.is_empty())
                        fallback=|| view! { <p class="empty">"No tasks are scheduled for you."</p> }
                    >
                        <div class="alloc scroll" node_ref=scroller on:scroll=move |_| measure()>
                            <table class="grid fixed alloctable">
                                <thead>
                                    <tr>
                                        <th class="name">"Task"</th>
                                        {move || days.get().into_iter().map(|d| view! {
                                            <th class:off=matches!(d.weekday(), Weekday::Sat | Weekday::Sun) class:today=d == today.get()>
                                                <span class="small">{format::weekday(d)}</span>
                                                <br />
                                                {if d.day() == 1 { format!("{} {}", d.day(), format::month(d)) } else { d.day().to_string() }}
                                            </th>
                                        }).collect_view()}
                                    </tr>
                                    <tr class="available">
                                        <th class="name">"Hours booked"</th>
                                        {move || hours.get().unwrap_or_default().into_iter().map(|(booked, capacity)| {
                                            let over = booked > u32::from(capacity);
                                            view! {
                                            <th class="booked" class:over=over>
                                                {(booked > 0 || capacity > 0).then(|| view! {
                                                    <HourBoxes hours=booked title=format!("{booked} of the {capacity} hours you have")  />
                                                })}
                                            </th>
                                        }}).collect_view()}
                                    </tr>
                                </thead>
                                <tbody>
                                    <For each=move || mine.get() key=|t| format!("{t:?}") children=move |task| {
                                        let (plan, id) = (task.plan, task.task);
                                        let title = said(&task);
                                        let colour = colour_of(id);
                                        let by_day = task.days.clone();
                                        let (from, to) = (task.start, task.end);
                                        view! {
                                            <tr class:picked=move || open.with(|o| o.is_some_and(|(_, n)| n.id() == id))>
                                                <td class="name" title=title>
                                                    <button type="button" class="plain" on:click=move |_| guard.then(Callback::new(move |()| open.set(Some((plan, PlanNode::Task(id))))))>
                                                        <strong>{task.name.clone()}</strong>
                                                        <span class="small">{task.plan_name.clone()}</span>
                                                    </button>
                                                </td>
                                                {move || days.get().into_iter().map(|d| {
                                                    let worked = by_day.get(&d).copied().unwrap_or(0);
                                                    let runs = d >= from && d <= to;
                                                    view! {
                                                        <td
                                                            class:off=matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
                                                            class:runs=runs
                                                            class:first=d == from
                                                            class:last=d == to
                                                            style=format!("--run:{colour}")
                                                        >
                                                            {(worked > 0).then(|| view! {
                                                                <HourBoxes hours=u32::from(worked) title=format!("{worked} h on {}", format::day(d)) />
                                                            })}
                                                        </td>
                                                    }
                                                }).collect_view()}
                                            </tr>
                                        }
                                    } />
                                </tbody>
                            </table>
                        </div>
                    </Show>
                </div>
                {move || {
                    open.get().map(|(plan, node)| view! {
                        <ItemPane
                            plan=plan
                            node=node
                            fresh=fresh
                            guard=guard
                            on_close=Callback::new(move |_| open.set(None))
                        />
                    })
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_always_has_the_same_colour() {
        let id = Uuid::from_u128(12345);
        assert_eq!(colour_of(id), colour_of(id));
        assert!(COLOURS.contains(&colour_of(Uuid::from_u128(7))));
    }
}
