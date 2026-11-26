//! What dragging handles on the chart does to the plan: adding and cutting dependencies, moving
//! items between groups and changing how long tasks and buffers run.

// modules {{{
use chrono::{Duration, NaiveDate};
use leptos::prelude::*;
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    planner::items::{
        dependency::PlanNode,
        parent::Parent,
        plan_buffer::PlanBufferPatch,
        task::{PersistentState, Status, TaskPatch},
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
};

use crate::{
    card,
    draft::{Draft, Pick},
    gantt::{
        links,
        shift::{Editing, Held, Pin},
    },
    guard::Guard,
    model,
    ui::use_sync,
};
// }}}

// helpers {{{
fn on(node: PlanNode) -> PlanNode {
    node
}

/// The days a task really started and ended, for one whose end is recorded
fn ended(state: PersistentState) -> Option<(NaiveDate, NaiveDate)> {
    match (state.status, state.actual_start_date, state.actual_end_date) {
        (Status::Completed | Status::OnHold | Status::Dropped, Some(start), Some(end)) => {
            Some((start, end.max(start)))
        }
        _ => None,
    }
}

fn with_dependency(deps: Vec<PlanNode>, node: PlanNode) -> Vec<PlanNode> {
    let mut deps = deps;
    if !deps.iter().any(|d| *d == on(node)) {
        deps.push(on(node));
    }
    deps
}

fn without_dependency(deps: Vec<PlanNode>, node: PlanNode) -> Vec<PlanNode> {
    deps.into_iter().filter(|d| *d != on(node)).collect()
}

/// What a resize changes: the days a task is to run, or for one that is done or on hold the day
/// it really ended, so its length is the days from its start to its end
fn duration_of(plan: &Plan, node: PlanNode) -> Option<u32> {
    match node {
        PlanNode::Task(id) => {
            let task = plan.tasks().get(&id)?;
            match ended(task.persistent_state()) {
                Some((start, end)) => Some((end - start).num_days() as u32 + 1),
                None => Some(task.duration()),
            }
        }
        PlanNode::PlanBuffer(id) => plan.plan_buffers().get(&id).map(|b| *b.duration()),
        _ => None,
    }
}

/// The group an item dropped on another ends up in: the group itself, else the group the other is in
fn parent_for(plan: &Plan, over: Option<PlanNode>) -> Parent {
    match over {
        None => Parent::Plan,
        Some(PlanNode::TaskGroup(id)) => Parent::Node(id),
        Some(other) => model::parent_of(plan, other),
    }
}

fn task_state(id: uuid::Uuid, state: PersistentState) -> PlanUpdate {
    PlanUpdate::Task(Crud::Update(TaskPatch {
        id,
        name: None,
        parent: None,
        duration: None,
        time_constraint: None,
        dependencies: None,
        allocations: None,
        affinities: None,
        actual_workers: None,
        persistent_state: Some(state),
    }))
}
// }}}

// editing {{{
pub fn editing(plan: ShortId) -> Editing {
    let sync = use_sync();
    let read = move |f: &dyn Fn(&Plan) -> Option<Vec<PlanUpdate>>| {
        sync.tenant
            .with_untracked(|t| f(t.as_ref()?.plans().get(&plan)?))
            .unwrap_or_default()
    };
    let send = move |updates: Vec<PlanUpdate>| {
        for update in updates {
            sync.plan(plan, update);
        }
    };
    let link = Callback::new(move |(node, other, right): (PlanNode, PlanNode, bool)| {
        // the right edge: the other waits on this item, the left: this item waits on the other
        let (waiting, before) = if right { (other, node) } else { (node, other) };
        send(read(&|p| {
            let deps = with_dependency(model::dependencies(p, waiting), before);
            Some(vec![PlanUpdate::set_dependencies(waiting, deps)])
        }));
    });
    let unlink = Callback::new(move |(selected, other): (PlanNode, PlanNode)| {
        send(read(&|p| {
            let (waiting, before) = if model::dependencies(p, selected)
                .iter()
                .any(|d| *d == on(other))
            {
                (selected, other)
            } else {
                (other, selected)
            };
            let deps = without_dependency(model::dependencies(p, waiting), before);
            Some(vec![PlanUpdate::set_dependencies(waiting, deps)])
        }));
    });
    let moved = Callback::new(move |(node, over): (PlanNode, Option<PlanNode>)| {
        let result = sync.tenant.with_untracked(|t| {
            let p = t.as_ref()?.plans().get(&plan)?;
            Some(p.moving(node, parent_for(p, over)))
        });
        match result {
            Some(Ok(updates)) => send(updates),
            Some(Err(why)) => sync.notice.set(Some(why)),
            None => {}
        }
    });
    let dated = Callback::new(move |node: PlanNode| {
        sync.tenant
            .with_untracked(|t| {
                let task = match node {
                    PlanNode::Task(id) => t.as_ref()?.plans().get(&plan)?.tasks().get(&id),
                    _ => None,
                };
                task.map(|t| t.persistent_state().status != Status::NotStarted)
            })
            .unwrap_or(false)
    });
    let shift_dates = Callback::new(move |(node, days): (PlanNode, i32)| {
        send(read(&|p| {
            let PlanNode::Task(id) = node else {
                return None;
            };
            let state = p.tasks().get(&id)?.persistent_state();
            let by = |d: NaiveDate| d.checked_add_signed(Duration::days(i64::from(days)));
            Some(vec![task_state(
                id,
                PersistentState {
                    actual_start_date: state.actual_start_date.and_then(by),
                    actual_end_date: state.actual_end_date.and_then(by),
                    ..state
                },
            )])
        }));
    });
    let restart = Callback::new(move |(node, days): (PlanNode, i32)| {
        send(read(&|p| {
            let PlanNode::Task(id) = node else {
                return None;
            };
            let state = p.tasks().get(&id)?.persistent_state();
            let start = state
                .actual_start_date?
                .checked_add_signed(Duration::days(i64::from(days)))?;
            Some(vec![task_state(
                id,
                PersistentState {
                    actual_start_date: Some(state.actual_end_date.map_or(start, |e| start.min(e))),
                    ..state
                },
            )])
        }));
    });
    let resize = Callback::new(move |(node, days): (PlanNode, i32)| {
        send(read(&|p| {
            // finished or paused work ends on another day, the rest takes more or fewer days
            if let PlanNode::Task(id) = node {
                let state = p.tasks().get(&id)?.persistent_state();
                if let Some((start, end)) = ended(state) {
                    let end = end
                        .checked_add_signed(Duration::days(i64::from(days)))?
                        .max(start);
                    return Some(vec![task_state(
                        id,
                        PersistentState {
                            actual_end_date: Some(end),
                            ..state
                        },
                    )]);
                }
            }
            let now = i64::from(duration_of(p, node)?);
            let duration = u32::try_from((now + i64::from(days)).max(1)).ok()?;
            Some(vec![match node {
                PlanNode::Task(id) => PlanUpdate::Task(Crud::Update(TaskPatch {
                    id,
                    name: None,
                    parent: None,
                    duration: Some(duration),
                    time_constraint: None,
                    dependencies: None,
                    allocations: None,
                    affinities: None,
                    actual_workers: None,
                    persistent_state: None,
                })),
                PlanNode::PlanBuffer(id) => PlanUpdate::PlanBuffer(Crud::Update(PlanBufferPatch {
                    id,
                    parent: None,
                    dependencies: None,
                    set_start_day: None,
                    duration: Some(duration),
                })),
                _ => return None,
            }])
        }));
    });
    let duration = Callback::new(move |node: PlanNode| {
        sync.tenant
            .with_untracked(|t| duration_of(t.as_ref()?.plans().get(&plan)?, node))
            .unwrap_or(1)
    });
    let chain = Callback::new(move |(a, b): (PlanNode, PlanNode)| {
        sync.tenant.with_untracked(|t| {
            t.as_ref()
                .and_then(|t| t.plans().get(&plan))
                .map(|p| links::chain(p, a, b))
                .unwrap_or_default()
        })
    });
    let pin = Callback::new(move |node: PlanNode| {
        sync.tenant
            .with_untracked(|t| Draft::of(t.as_ref()?.plans().get(&plan)?, node)?.pin())
    });
    Editing {
        pin,
        // only an item being edited has a day to drag and dots to click, see `staging`
        constrain: Callback::new(|_| ()),
        hold: Callback::new(|_| ()),
        daying: Callback::new(|_| ()),
        pick: Callback::new(|_| ()),
        dated,
        shift_dates,
        restart,
        link,
        unlink,
        moved,
        resize,
        duration,
        chain,
    }
}
// }}}

// staging {{{
/// The same as `live`, but what is asked of the item being edited goes in its draft, to be saved
/// or discarded with the rest of it
pub fn staging(
    live: Editing,
    plan: ShortId,
    draft: RwSignal<Option<Draft>>,
    guard: Guard,
) -> Editing {
    let sync = use_sync();
    let ours = move |node: PlanNode| {
        draft.with_untracked(|d| d.as_ref().is_some_and(|d| !d.new && d.node == node))
    };
    let stage = move |f: &dyn Fn(&Plan, &mut Draft) -> Result<(), String>| {
        let mut said = None;
        sync.tenant.with_untracked(|t| {
            if let Some(p) = t.as_ref().and_then(|t| t.plans().get(&plan)) {
                draft.update(|d| said = d.as_mut().and_then(|d| f(p, d).err()));
            }
        });
        guard.dirtied();
        if said.is_some() {
            sync.notice.set(said);
        }
    };
    let link = Callback::new(move |(node, other, right): (PlanNode, PlanNode, bool)| {
        // the right edge: the other waits on this item, the left: this item waits on the other
        let side = |after: bool| if after { Pick::After } else { Pick::Before };
        if ours(node) {
            stage(&|p, d| d.link(p, side(right), other));
        } else if ours(other) {
            stage(&|p, d| d.link(p, side(!right), node));
        } else {
            live.link.run((node, other, right));
        }
    });
    let unlink = Callback::new(move |(selected, other): (PlanNode, PlanNode)| {
        let them = match (ours(selected), ours(other)) {
            (true, _) => other,
            (_, true) => selected,
            _ => return live.unlink.run((selected, other)),
        };
        stage(&|p, d| match d.dependencies.contains(&them) {
            true => d.toggle(p, Pick::Before, them),
            false if d.dependents.contains(&them) => d.toggle(p, Pick::After, them),
            false => Ok(()),
        });
    });
    let moved = Callback::new(
        move |(node, over): (PlanNode, Option<PlanNode>)| match ours(node) {
            true => stage(&|p, d| d.place(p, over)),
            false => live.moved.run((node, over)),
        },
    );
    // whether the days it really ran have an end, which its right grip then moves
    let finished = |p: &Plan, node: PlanNode| match node {
        PlanNode::Task(id) => p
            .tasks()
            .get(&id)
            .is_some_and(|t| ended(t.persistent_state()).is_some()),
        _ => false,
    };
    let shift_dates = Callback::new(move |(node, days): (PlanNode, i32)| match ours(node) {
        true => stage(&|_, d| {
            d.moved = (d.moved.0 + days, d.moved.1 + days);
            Ok(())
        }),
        false => live.shift_dates.run((node, days)),
    });
    let restart = Callback::new(move |(node, days): (PlanNode, i32)| match ours(node) {
        true => stage(&|_, d| {
            d.moved.0 += days;
            Ok(())
        }),
        false => live.restart.run((node, days)),
    });
    let resize = Callback::new(move |(node, days): (PlanNode, i32)| match ours(node) {
        true => stage(&|p, d| {
            if finished(p, d.node) {
                d.moved.1 += days;
                return Ok(());
            }
            let least = card::least_days(d.node);
            let grown = |now: u32| (i64::from(now) + i64::from(days)).clamp(least, card::MOST_DAYS);
            d.duration = d.duration.map(|now| grown(now) as u32);
            Ok(())
        }),
        false => live.resize.run((node, days)),
    });
    let duration = Callback::new(move |node: PlanNode| {
        let saved = live.duration.run(node);
        let Some(d) = draft.get_untracked().filter(|_| ours(node)) else {
            return saved;
        };
        let ended = sync.tenant.with_untracked(|t| {
            let plan = t.as_ref().and_then(|t| t.plans().get(&plan));
            plan.is_some_and(|p| finished(p, node))
        });
        match ended {
            true => (i64::from(saved) + i64::from(d.moved.1 - d.moved.0)).max(1) as u32,
            false => d.duration.unwrap_or(saved),
        }
    });
    // a click on a dot of the item makes clicks on others link and unlink them, another ends it
    let pick = Callback::new(move |(node, right): (PlanNode, bool)| {
        if ours(node) {
            let side = if right { Pick::After } else { Pick::Before };
            draft.update(|d| {
                if let Some(d) = d {
                    d.pick = Some(side).filter(|s| d.pick != Some(*s));
                }
            });
        }
    });
    // the held day is put on a day, which ends the picking of one
    let constrain = Callback::new(move |(node, end, day): (PlanNode, bool, NaiveDate)| {
        if ours(node) {
            stage(&|_, d| {
                let held = d
                    .pin()
                    .map_or(if end { Held::Latest } else { Held::Earliest }, |p| p.held);
                d.hold(Some(Pin { day, held, end }));
                d.day_set = true;
                d.pick = d.pick.filter(|p| *p != Pick::Day);
                Ok(())
            });
        }
    });
    let hold = Callback::new(move |(node, held): (PlanNode, Option<Held>)| {
        if ours(node) {
            stage(&|_, d| {
                let pin = d.pin().zip(held).map(|(pin, held)| Pin { held, ..pin });
                d.hold(pin);
                Ok(())
            });
        }
    });
    let daying = Callback::new(move |node: PlanNode| {
        if ours(node) {
            draft.update(|d| {
                if let Some(d) = d.as_mut().filter(|d| d.pin().is_some()) {
                    d.pick = Some(Pick::Day).filter(|p| d.pick != Some(*p));
                }
            });
        }
    });
    Editing {
        constrain,
        hold,
        daying,
        pick,
        link,
        unlink,
        moved,
        shift_dates,
        restart,
        resize,
        duration,
        ..live
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn dependencies_are_added_once_and_cutting_the_last_leaves_none() {
        let a = PlanNode::Task(Uuid::from_u128(1));
        let deps = with_dependency(Vec::new(), a);
        assert_eq!(deps.len(), 1);
        assert_eq!(with_dependency(deps.clone(), a), deps);
        assert!(without_dependency(without_dependency(deps, a), a).is_empty());
    }
}
// }}}
