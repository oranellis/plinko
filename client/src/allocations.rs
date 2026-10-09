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
const DAY_WIDTH: f64 = 36.0;
/// How wide the column of names starts, and the least and most it can be dragged to
const NAME_WIDTH: f64 = 240.0;
const NAME_LEAST: f64 = 120.0;
const NAME_MOST: f64 = 600.0;
/// How close to the edge of the names a press has to be to take hold of it
const GRIP: f64 = 5.0;
/// The weeks shown from before the current one
const BEFORE: u64 = 2;
/// The fewest weeks shown, and how many more than the scroll has reached
const LEAST: u64 = 12;
const AHEAD: u64 = 4;
/// The hours a day holds, which the pies and bars are drawn against
const FULL_DAY: u8 = 8;

/// A day as a pie of eight slices, one to an hour, with the hours booked filled in. A day with
/// nothing booked has no pie. The slices are green when the hours booked are all the hours the
/// person has, orange while there are still hours left, and red when they are overbooked.
#[component]
fn HourPie(booked: u32, capacity: u8, #[prop(into)] title: String) -> impl IntoView {
    let colour = if booked > u32::from(capacity) {
        "#e57373"
    } else if booked == u32::from(capacity) {
        "#43c463"
    } else {
        "#c98a3c"
    };
    let slices = (0..FULL_DAY)
        .map(|i| {
            let fill = if u32::from(i) < booked {
                colour
            } else {
                "#2f2f31"
            };
            view! { <path d=slice(i) fill=fill stroke=fill stroke-width="0.8" stroke-linejoin="round" /> }
        })
        .collect_view();
    view! {
        <svg class="pie" width="26" height="26" viewBox="0 0 26 26">
            <title>{title}</title>
            {slices}
        </svg>
    }
}

/// The path of slice `i` of eight, which starts at the top and goes round clockwise
fn slice(i: u8) -> String {
    let (centre, radius) = (13.0_f64, 11.0_f64);
    let at = |turn: f64| {
        let angle =
            (turn / f64::from(FULL_DAY)) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
        (centre + radius * angle.cos(), centre + radius * angle.sin())
    };
    let ((x0, y0), (x1, y1)) = (at(f64::from(i)), at(f64::from(i) + 1.0));
    format!("M{centre} {centre} L{x0:.2} {y0:.2} A{radius} {radius} 0 0 1 {x1:.2} {y1:.2} Z")
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
    let name_width = RwSignal::new(NAME_WIDTH);
    // dragging the edge of the names
    let dragging = RwSignal::new(false);
    let near_edge = move |ev: &leptos::ev::PointerEvent| {
        let left = scroller
            .get_untracked()
            .map_or(0.0, |s| s.get_bounding_client_rect().left());
        (f64::from(ev.client_x()) - left - name_width.get_untracked()).abs() <= GRIP
    };
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
            let days = ((reached - name_width.get_untracked()) / DAY_WIDTH).max(0.0) as u64;
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
                        <div
                            class="alloc scroll"
                            class:grabbing=move || dragging.get()
                            style=move || format!("--name-w:{}px", name_width.get())
                            node_ref=scroller
                            on:scroll=move |_| measure()
                            on:pointerdown=move |ev| {
                                if near_edge(&ev) {
                                    dragging.set(true);
                                    if let Some(s) = scroller.get_untracked() {
                                        let _ = s.set_pointer_capture(ev.pointer_id());
                                    }
                                    ev.prevent_default();
                                }
                            }
                            on:pointermove=move |ev| {
                                if dragging.get_untracked() {
                                    let left = scroller.get_untracked().map_or(0.0, |s| s.get_bounding_client_rect().left());
                                    name_width.set((f64::from(ev.client_x()) - left).clamp(NAME_LEAST, NAME_MOST));
                                } else if let Some(s) = scroller.get_untracked() {
                                    let _ = s.class_list().toggle_with_force("edge", near_edge(&ev));
                                }
                            }
                            on:pointerup=move |ev| {
                                if dragging.get_untracked() {
                                    dragging.set(false);
                                    if let Some(s) = scroller.get_untracked() {
                                        let _ = s.release_pointer_capture(ev.pointer_id());
                                    }
                                    measure();
                                }
                            }
                        >
                            <table class="grid fixed alloctable">
                                <colgroup>
                                    <col style="width:var(--name-w)" />
                                    {move || days.get().into_iter().map(|_| view! { <col style=format!("width:{DAY_WIDTH}px") /> }).collect_view()}
                                </colgroup>
                                <thead>
                                    <tr class="months">
                                        <th class="name"></th>
                                        {move || {
                                            let days = days.get();
                                            days.iter().enumerate().map(|(i, d)| {
                                                // a month is named once, over all of its days that are shown
                                                let starts = i == 0 || d.day() == 1;
                                                let shown = days[i..].iter().take_while(|x| x.month() == d.month() && x.year() == d.year()).count();
                                                let label = format!("{} {}", format::month(*d), d.year());
                                                view! {
                                                    <th class="mcell" class:starts=starts class:m1=d.day() == 1>
                                                        {starts.then(|| view! {
                                                            <span class="monthbox" style=format!("width:{}px", shown as f64 * DAY_WIDTH)>
                                                                <span class="monthname">{label}</span>
                                                            </span>
                                                        })}
                                                    </th>
                                                }
                                            }).collect_view()
                                        }}
                                    </tr>
                                    <tr class="daynames">
                                        <th class="name">"Task"</th>
                                        {move || days.get().into_iter().map(|d| view! {
                                            <th class:off=matches!(d.weekday(), Weekday::Sat | Weekday::Sun) class:today=d == today.get() class:m1=d.day() == 1>
                                                {d.day()}
                                            </th>
                                        }).collect_view()}
                                    </tr>
                                    <tr class="available">
                                        <th class="name">"Hours booked"</th>
                                        {move || days.get().into_iter().zip(hours.get().unwrap_or_default()).map(|(d, (booked, capacity))| {
                                            view! {
                                            <th class="booked" class:off=matches!(d.weekday(), Weekday::Sat | Weekday::Sun) class:today=d == today.get() class:m1=d.day() == 1>
                                                {(booked > 0).then(|| view! {
                                                    <HourPie booked=booked capacity=capacity title=format!("{booked} of the {capacity} hours you have") />
                                                })}
                                            </th>
                                        }}).collect_view()}
                                    </tr>
                                </thead>
                                <tbody>
                                    <For each=move || mine.get() key=|t| format!("{t:?}") children=move |task| {
                                        let (plan, id) = (task.plan, task.task);
                                        let title = said(&task);
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
                                                    // the day's hours fill from the bottom of the task's bar
                                                    let filled = f64::from(worked.min(FULL_DAY)) / f64::from(FULL_DAY) * 100.0;
                                                    view! {
                                                        <td
                                                            class:off=matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
                                                            class:runs=runs
                                                            class:today=d == today.get_untracked()
                                                            class:m1=d.day() == 1
                                                            class:first=d == from
                                                            class:last=d == to
                                                            title=format!("{worked} h on {}", format::day(d))
                                                        >
                                                            {runs.then(|| view! {
                                                                <div class="runbar">
                                                                    <i class="fill" style=format!("height:{filled}%")></i>
                                                                </div>
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
    fn slices_go_round_from_the_top() {
        assert!(slice(0).starts_with("M13 13 L13.00 2.00 A11 11 0 0 1 "));
        assert_ne!(slice(0), slice(7));
        assert!(slice(2).contains("L24.00 13.00"));
    }
}
