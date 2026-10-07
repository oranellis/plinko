//! What you are doing, day by day, across every plan.

use chrono::{Days, NaiveDate};
use leptos::prelude::*;

use crate::{
    format,
    nav::Route,
    ui::use_sync,
};

const STEP: u64 = 14;

#[derive(Clone, PartialEq)]
struct Entry {
    plan: plinko::types::short_id::ShortId,
    task: uuid::Uuid,
    name: String,
    plan_name: String,
    hours: u8,
}

#[derive(Clone, PartialEq)]
struct Day {
    date: NaiveDate,
    capacity: u8,
    booked: u32,
    entries: Vec<Entry>,
}

#[component]
pub fn Work() -> impl IntoView {
    let sync = use_sync();
    let shown = RwSignal::new(STEP);
    let days = Memo::new(move |_| {
        let me = sync.me.get()?.id;
        let schedule = sync.schedule.get();
        sync.tenant.with(|t| {
            let t = t.as_ref()?;
            let today = t.today();
            let until = today + Days::new(shown.get());
            let calendar = t.users().get(&me)?.availability().calendar(today, until);
            let assignments = &schedule.as_ref()?.1.users.get(&me)?.assignments;
            let days = assignments
                .range(today..until)
                .map(|(date, list)| {
                    let entries = list
                        .iter()
                        .map(|a| {
                            let plan = t.plans().get(&a.plan);
                            Entry {
                                plan: a.plan,
                                task: a.task,
                                name: plan
                                    .and_then(|p| p.tasks().get(&a.task))
                                    .map_or("Busy on a plan you cannot see".to_string(), |t| {
                                        t.name().to_string()
                                    }),
                                plan_name: plan.map(|p| p.name().to_string()).unwrap_or_default(),
                                hours: a.hours.get(),
                            }
                        })
                        .collect();
                    Day {
                        date: *date,
                        capacity: calendar.on(*date).map_or(0, |h| h.get()),
                        booked: list.iter().map(|a| u32::from(a.hours.get())).sum(),
                        entries,
                    }
                })
                .collect::<Vec<_>>();
            Some(days)
        })
    });
    view! {
        <div class="bar">
            <h1>"My work"</h1>
            <span class="small">{move || if sync.recalculating() { "Recalculating…" } else { "" }}</span>
        </div>
        <Show
            when=move || days.get().is_some_and(|d| !d.is_empty())
            fallback=|| view! { <p class="empty">"Nothing is scheduled for you in the next two weeks."</p> }
        >
            <ul class="agenda">
                {move || {
                    days.get()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|day| {
                            let over = day.booked > u32::from(day.capacity);
                            view! {
                                <li>
                                    <section class:over=over>
                                        <h3>
                                            {format::day(day.date)}
                                            {format!(" · {} of {} hours", day.booked, day.capacity)}
                                        </h3>
                                        <ul class="items">
                                            {day.entries.into_iter().map(|e| view! {
                                                <li>
                                                    <a class="item" href=Route::Plan(e.plan, crate::nav::default_tab(), Some(e.task)).path()>
                                                        <strong>{e.name}</strong>
                                                        <span class="dates">{format!("{} · {} h", e.plan_name, e.hours)}</span>
                                                    </a>
                                                </li>
                                            }).collect_view()}
                                        </ul>
                                    </section>
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>
        </Show>
        <button type="button" on:click=move |_| shown.update(|n| *n += STEP)>"Show more"</button>
    }
}
