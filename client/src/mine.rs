//! The tasks the person signed in is down to do, across every plan they can see.

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
    /// The last day it runs
    pub end: NaiveDate,
    /// Hours worked on each day it runs
    pub days: BTreeMap<NaiveDate, u8>,
}

// use_my_tasks {{{
/// Every task scheduled with the person signed in on it, soonest first
pub fn use_my_tasks() -> Memo<Vec<MyTask>> {
    use_tasks_of(None)
}
// }}}

// use_tasks_of {{{
/// Every task scheduled with `who` on it, or the person signed in, soonest first
pub fn use_tasks_of(who: Option<Uuid>) -> Memo<Vec<MyTask>> {
    let sync = use_sync();
    Memo::new(move |_| {
        let Some(me) = who.or_else(|| sync.me.with(|m| m.as_ref().map(|m| m.id))) else {
            return Vec::new();
        };
        let mut mine = sync.schedule.with(|schedule| {
            let Some((_, schedule)) = schedule.as_ref() else {
                return Vec::new();
            };
            sync.tenant.with(|t| {
                let Some(t) = t.as_ref() else {
                    return Vec::new();
                };
                let mut mine = Vec::new();
                for (plan_id, state) in &schedule.plans {
                    let Some(plan) = t.plans().get(plan_id) else {
                        continue;
                    };
                    for (id, volatile) in &state.tasks {
                        let Some(days) = volatile.assignments.get(&Assignee::User(me)) else {
                            continue;
                        };
                        let Some(task) = plan.tasks().get(id) else {
                            continue;
                        };
                        let range = volatile.date_range;
                        mine.push(MyTask {
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
                mine
            })
        });
        mine.sort_by(|a, b| {
            (a.start, a.end, &a.name, a.task).cmp(&(b.start, b.end, &b.name, b.task))
        });
        mine
    })
}
// }}}

// set_status {{{
/// Moves a task to a new status, the server fills in the dates that goes with it
pub fn set_status(plan: ShortId, task: Uuid, status: Status) {
    let sync = use_sync();
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
