//! What dragging handles on the chart does to the plan: adding and cutting dependencies, moving
//! items between groups and changing how long tasks and buffers run.

use leptos::prelude::*;
use plinko::{
    planner::{plan::Plan, plan_update::PlanUpdate},
    types::{
        crud::Crud,
        dependency::{Dependency, DependencyNode, PlanNode},
        parent::Parent,
        plan_buffer::PlanBufferPatch,
        short_id::ShortId,
        task::TaskPatch,
    },
};

use crate::{
    gantt::{links, shift::Editing},
    model,
    ui::use_sync,
};

fn on(node: PlanNode) -> DependencyNode {
    DependencyNode::Node(node)
}

fn with_dependency(deps: Vec<Dependency>, node: PlanNode) -> Vec<Dependency> {
    let mut deps = deps;
    if !deps.iter().any(|d| d.node == on(node)) {
        deps.push(Dependency {
            node: on(node),
            lag: 0,
        });
    }
    deps
}

fn without_dependency(deps: Vec<Dependency>, node: PlanNode) -> Vec<Dependency> {
    let mut deps: Vec<Dependency> = deps.into_iter().filter(|d| d.node != on(node)).collect();
    if deps.is_empty() {
        deps.push(Dependency::default());
    }
    deps
}

fn duration_of(plan: &Plan, node: PlanNode) -> Option<u32> {
    match node {
        PlanNode::Task(id) => plan.tasks().get(&id).map(|t| t.duration()),
        PlanNode::PlanBuffer(id) => plan.plan_buffers().get(&id).map(|b| *b.duration()),
        _ => None,
    }
}

/// The group an item dropped on another ends up in: the group itself, else the group the other is in
fn parent_for(plan: &Plan, over: Option<PlanNode>) -> Parent {
    match over {
        None => Parent::PlanStart,
        Some(PlanNode::TaskGroup(id)) => Parent::Node(id),
        Some(other) => model::parent_of(plan, other),
    }
}

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
                .any(|d| d.node == on(other))
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
    let resize = Callback::new(move |(node, days): (PlanNode, i32)| {
        send(read(&|p| {
            let now = i64::from(duration_of(p, node)?);
            let duration = u32::try_from((now + i64::from(days)).max(1)).ok()?;
            Some(vec![match node {
                PlanNode::Task(id) => PlanUpdate::Task(Crud::Update(TaskPatch {
                    id,
                    name: None,
                    parent: None,
                    mode: None,
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
    Editing {
        link,
        unlink,
        moved,
        resize,
        duration,
        chain,
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn dependencies_are_added_once_and_cutting_the_last_leaves_the_start() {
        let a = PlanNode::Task(Uuid::from_u128(1));
        let deps = with_dependency(vec![Dependency::default()], a);
        assert_eq!(deps.len(), 2);
        assert_eq!(with_dependency(deps.clone(), a), deps);
        assert_eq!(
            without_dependency(without_dependency(deps, a), a),
            vec![Dependency::default()]
        );
    }
}
