//! Where you land: what you are doing today and next, and the plans you go to most.

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::types::task::Status;

use crate::{
    format,
    icons::{Icon, IconKind},
    mine::{MyTask, set_status, use_my_tasks},
    nav::{PlanTab, Route},
    plans::{PlanCards, use_cards},
    ui::use_sync,
};

/// How many tasks still to start are shown
const UPCOMING: usize = 4;

#[component]
pub fn Home() -> impl IntoView {
    let sync = use_sync();
    let today = move || {
        sync.tenant
            .with(|t| t.as_ref().map(|t| t.today()))
            .unwrap_or_default()
    };
    let mine = use_my_tasks();
    // the most overdue come first in both lists
    let doing = Memo::new(move |_| {
        let mut list: Vec<MyTask> = mine.with(|m| {
            m.iter()
                .filter(|t| matches!(t.status, Status::InProgress))
                .cloned()
                .collect()
        });
        list.sort_by_key(|t| (t.end, t.start));
        list
    });
    let next = Memo::new(move |_| {
        let mut list: Vec<MyTask> = mine.with(|m| {
            m.iter()
                .filter(|t| matches!(t.status, Status::NotStarted))
                .cloned()
                .collect()
        });
        list.sort_by_key(|t| (t.start, t.end));
        list.truncate(UPCOMING);
        list
    });
    let cards = use_cards();
    let quick = Memo::new(move |_| {
        let all = cards.get();
        let starred: Vec<_> = all.iter().filter(|c| c.favourite).cloned().collect();
        if starred.is_empty() {
            all.into_iter().take(4).collect()
        } else {
            starred
        }
    });
    view! {
        <div class="home">
            <h1>"Home"</h1>
            <section>
                <h2>"Work Today"</h2>
                <Show
                    when=move || doing.with(|d| !d.is_empty())
                    fallback=|| view! { <p class="small">"Nothing is in progress."</p> }
                >
                    <ul class="work">
                        <For each=move || doing.get() key=|t| format!("{t:?}") children=move |t| view! { <Row task=t today=today() /> } />
                    </ul>
                </Show>
            </section>
            <section>
                <h2>"Upcoming Work"</h2>
                <Show
                    when=move || next.with(|d| !d.is_empty())
                    fallback=|| view! { <p class="small">"Nothing is waiting to start."</p> }
                >
                    <ul class="work">
                        <For each=move || next.get() key=|t| format!("{t:?}") children=move |t| view! { <Row task=t today=today() /> } />
                    </ul>
                </Show>
            </section>
            <section>
                <h2>"Quick Access Plans"</h2>
                <PlanCards cards=quick />
                <a href=Route::Plans.path()>"All plans"</a>
            </section>
        </div>
    }
}

/// A task with the button that moves it on: finish what is in progress, start what is waiting
#[component]
fn Row(task: MyTask, today: NaiveDate) -> impl IntoView {
    let running = matches!(task.status, Status::InProgress);
    let late = if running {
        task.end < today
    } else {
        task.start < today
    };
    let (plan, id) = (task.plan, task.task);
    let (title, next, icon, class) = if running {
        (
            "Mark complete",
            Status::Completed,
            IconKind::Tick,
            "quick done",
        )
    } else {
        ("Start", Status::InProgress, IconKind::Play, "quick start")
    };
    view! {
        <li class="row-task">
            <button
                type="button"
                class=class
                title=title
                aria-label=title
                on:click=move |_| set_status(plan, id, next)
            >
                <Icon kind=icon size=16 />
            </button>
            <a class="item" href=Route::Plan(plan, PlanTab::Gantt, Some(id)).path()>
                <strong>{task.name.clone()}</strong>
                <span class="dates" class:late=late>
                    {format!(
                        "{} · {} to {}{}",
                        task.plan_name,
                        format::day(task.start),
                        format::day(task.end),
                        if late { " · overdue" } else { "" },
                    )}
                </span>
            </a>
        </li>
    }
}
