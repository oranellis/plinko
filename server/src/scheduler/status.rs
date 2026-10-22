// modules {{{
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    planner::items::{
        dependency::{Dependency, PlanNode},
        milestone::DerivedStatus,
        parent::Parent,
        task::Status,
    },
    planner::plan::Plan,
};
// }}}

// milestone_statuses {{{
/// The derived status of every milestone in the plan. It follows what the milestone depends on,
/// not the schedule, so it is known before anything is placed.
pub fn milestone_statuses(plan: &Plan) -> HashMap<Uuid, DerivedStatus> {
    let mut derive = Derive {
        plan,
        done: HashMap::new(),
        active: HashMap::new(),
    };
    plan.milestones()
        .iter()
        .map(|(id, m)| {
            let parent = m.parent();
            let status = if derive.all_done(m.dependencies(), parent) {
                DerivedStatus::Completed
            } else if derive.any_active(m.dependencies(), parent) {
                DerivedStatus::InProgress
            } else {
                DerivedStatus::NotStarted
            };
            (*id, status)
        })
        .collect()
}
// }}}

struct Derive<'a> {
    plan: &'a Plan,
    done: HashMap<PlanNode, bool>,
    active: HashMap<PlanNode, bool>,
}

// impl Derive {{{
impl Derive<'_> {
    fn deps_of(&self, node: PlanNode) -> (&[Dependency], Parent) {
        let plan = self.plan;
        match node {
            PlanNode::Task(id) => (plan.tasks()[&id].dependencies(), plan.tasks()[&id].parent()),
            PlanNode::Milestone(id) => {
                let m = &plan.milestones()[&id];
                (m.dependencies(), m.parent())
            }
            PlanNode::PlanBuffer(id) => {
                let b = &plan.plan_buffers()[&id];
                (b.dependencies(), b.parent())
            }
            PlanNode::PlanDelay(id) => {
                let d = &plan.plan_delays()[&id];
                (d.dependencies(), d.parent())
            }
            PlanNode::TaskGroup(id) => {
                let g = &plan.task_groups()[&id];
                (g.dependencies(), g.parent())
            }
        }
    }

    /// The items inside a group
    fn members(&self, group: Uuid) -> Vec<PlanNode> {
        let plan = self.plan;
        let parent = Parent::Node(group);
        let tasks = plan.tasks().values().filter(|t| t.parent() == parent);
        let milestones = plan.milestones().values().filter(|m| m.parent() == parent);
        let buffers = plan
            .plan_buffers()
            .values()
            .filter(|b| b.parent() == parent);
        let delays = plan.plan_delays().values().filter(|d| d.parent() == parent);
        let groups = plan.task_groups().values().filter(|g| g.parent() == parent);
        tasks
            .map(|t| PlanNode::Task(*t.id()))
            .chain(milestones.map(|m| PlanNode::Milestone(*m.id())))
            .chain(buffers.map(|b| PlanNode::PlanBuffer(*b.id())))
            .chain(delays.map(|d| PlanNode::PlanDelay(*d.id())))
            .chain(groups.map(|g| PlanNode::TaskGroup(*g.id())))
            .collect()
    }

    /// What `ParentStart` stands for: the dependencies of the group around the item, if any
    fn parent_start(&self, parent: Parent) -> Option<(Vec<Dependency>, Parent)> {
        let Parent::Node(group) = parent else {
            return None;
        };
        let (own, grandparent) = self.deps_of(PlanNode::TaskGroup(group));
        Some((own.to_vec(), grandparent))
    }

    fn all_done(&mut self, deps: &[Dependency], parent: Parent) -> bool {
        deps.iter().all(|d| match *d {
            Dependency::Node(n) => self.is_done(n),
            Dependency::ParentStart => match self.parent_start(parent) {
                Some((own, grandparent)) => self.all_done(&own, grandparent),
                None => true,
            },
        })
    }

    fn any_active(&mut self, deps: &[Dependency], parent: Parent) -> bool {
        deps.iter().any(|d| match *d {
            Dependency::Node(n) => self.is_active(n),
            Dependency::ParentStart => match self.parent_start(parent) {
                Some((own, grandparent)) => self.any_active(&own, grandparent),
                None => false,
            },
        })
    }

    fn is_done(&mut self, node: PlanNode) -> bool {
        if let Some(known) = self.done.get(&node) {
            return *known;
        }
        let (deps, parent) = self.deps_of(node);
        let deps = deps.to_vec();
        let done = match node {
            PlanNode::Task(id) => matches!(
                self.plan.tasks()[&id].persistent_state().status,
                Status::Completed | Status::Dropped
            ),
            PlanNode::Milestone(_) | PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => {
                self.all_done(&deps, parent)
            }
            PlanNode::TaskGroup(id) => {
                self.all_done(&deps, parent)
                    && self.members(id).into_iter().all(|m| self.is_done(m))
            }
        };
        self.done.insert(node, done);
        done
    }

    /// Whether a task in progress is reached directly or through buffers and groups, milestones
    /// do not pass it on
    fn is_active(&mut self, node: PlanNode) -> bool {
        if let Some(known) = self.active.get(&node) {
            return *known;
        }
        let (deps, parent) = self.deps_of(node);
        let deps = deps.to_vec();
        let active = match node {
            PlanNode::Task(id) => {
                self.plan.tasks()[&id].persistent_state().status == Status::InProgress
            }
            PlanNode::Milestone(_) => false,
            PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => self.any_active(&deps, parent),
            PlanNode::TaskGroup(id) => {
                self.any_active(&deps, parent)
                    || self.members(id).into_iter().any(|m| self.is_active(m))
            }
        };
        self.active.insert(node, active);
        active
    }
}
// }}}
