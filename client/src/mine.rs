//! A person's scheduled tasks across every plan.

// modules {{{
use std::collections::BTreeMap;

use chrono::NaiveDate;
use leptos::prelude::*;
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    planner::items::task::{Assignee, PersistentState, Status, TaskPatch},
    planner::plan_update::PlanUpdate,
};
use uuid::Uuid;

use crate::ui::use_sync;
// }}}

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct MyTask {
    pub plan: ShortId,
    pub plan_name: String,
    pub task: Uuid,
    pub name: String,
    pub status: Status,
    pub start: NaiveDate,
    /// Inclusive
    pub end: NaiveDate,
    /// Hours on each day
    pub days: BTreeMap<NaiveDate, u8>,
}

// use_my_tasks {{{
pub fn use_my_tasks() -> Memo<Vec<MyTask>> {
    use_tasks_of(None)
}
// }}}

// use_tasks_of {{{
/// Tasks scheduled for `who`, or the signed-in person when `None`, soonest first
pub fn use_tasks_of(who: Option<Uuid>) -> Memo<Vec<MyTask>> {
    let sync = use_sync();
    Memo::new(move |_| {
        let Some(person) = who.or_else(|| sync.me.with(|m| m.as_ref().map(|m| m.id))) else {
            return Vec::new();
        };
        let mut tasks = sync.schedule.with(|schedule| {
            let Some((_, schedule)) = schedule.as_ref() else {
                return Vec::new();
            };
            sync.tenant.with(|t| {
                let Some(t) = t.as_ref() else {
                    return Vec::new();
                };
                let mut tasks = Vec::new();
                for (plan_id, state) in &schedule.plans {
                    let Some(plan) = t.plans().get(plan_id) else {
                        continue;
                    };
                    for (id, volatile) in &state.tasks {
                        let Some(days) = volatile.assignments.get(&Assignee::User(person)) else {
                            continue;
                        };
                        let Some(task) = plan.tasks().get(id) else {
                            continue;
                        };
                        let range = volatile.date_range;
                        tasks.push(MyTask {
                            plan: *plan_id,
                            plan_name: plan.name().to_string(),
                            task: *id,
                            name: task.name().to_string(),
                            status: task.persistent_state().status,
                            start: range.start(),
                            end: range.end().pred_opt().unwrap_or(range.start()),
                            days: days.iter().map(|(d, h)| (*d, h.get())).collect(),
                        });
                    }
                }
                tasks
            })
        });
        tasks.sort_by(|a, b| {
            (a.start, a.end, &a.name, a.task).cmp(&(b.start, b.end, &b.name, b.task))
        });
        tasks
    })
}
// }}}

// set_status {{{
/// The server fills in the dates that go with the new status
pub fn set_status(plan: ShortId, task: Uuid, status: Status) {
    set_status_with(use_sync(), plan, task, status);
}

/// For callers outside a component, such as a key handler
pub fn set_status_with(sync: crate::sync::Sync, plan: ShortId, task: Uuid, status: Status) {
    let Some(was) = sync.tenant.with_untracked(|t| {
        Some(
            t.as_ref()?
                .plans()
                .get(&plan)?
                .tasks()
                .get(&task)?
                .persistent_state(),
        )
    }) else {
        return;
    };
    let patch = TaskPatch {
        id: task,
        name: None,
        parent: None,
        duration: None,
        time_constraint: None,
        dependencies: None,
        allocations: None,
        affinities: None,
        actual_workers: None,
        persistent_state: Some(PersistentState { status, ..was }),
    };
    sync.plan(plan, PlanUpdate::Task(Crud::Update(patch)));
}
// }}}
