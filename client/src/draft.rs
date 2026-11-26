//! An item being created or edited. What the card and the chart change about it is kept here and
//! only reaches the plan when it is saved, so it can be discarded as a whole.

// modules {{{
use chrono::{Duration, NaiveDate};
use leptos::prelude::*;
use plinko::{
    common::{crud::Crud, short_id::ShortId},
    planner::items::{
        dependency::PlanNode,
        milestone::NewMilestone,
        parent::Parent,
        plan_buffer::NewPlanBuffer,
        plan_delay::NewPlanDelay,
        task::{NewTask, PersistentState},
        task_group::NewTaskGroup,
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
};

use crate::{model, sync::Sync};
// }}}

// constants {{{
/// The days a new task, buffer or delay starts with
pub const NEW_DAYS: u32 = 3;
// }}}

/// What of an item is set from the card: where it is and how long it lasts
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub node: PlanNode,
    /// Not in the plan yet, so saving adds it
    pub new: bool,
    pub parent: Parent,
    pub dependencies: Vec<PlanNode>,
    /// What waits for it
    pub dependents: Vec<PlanNode>,
    /// Its days, for the kinds that have them
    pub duration: Option<u32>,
    /// The days the day it really started and the day it really ended are moved by
    pub moved: (i32, i32),
    /// The handle of the card that is lit, which clicks on the chart then answer to
    pub pick: Option<Pick>,
}

// Draft supporting types {{{
/// A handle of the card that can be lit
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// The item waits for what is clicked
    Before,
    /// What is clicked waits for the item
    After,
    /// The item goes in the group clicked, or the group of the item clicked
    Group,
}

/// The draft of the page, for the card, the chart and the forms to share
#[derive(Clone, Copy)]
pub struct Drafting(pub RwSignal<Option<Draft>>);
// }}}

// impl Draft {{{
impl Draft {
    /// The draft of an item that is in the plan, as it is
    pub fn of(plan: &Plan, node: PlanNode) -> Option<Self> {
        let duration = match node {
            PlanNode::Task(id) => Some(plan.tasks().get(&id)?.duration()),
            PlanNode::PlanBuffer(id) => Some(*plan.plan_buffers().get(&id)?.duration()),
            PlanNode::PlanDelay(id) => Some(*plan.plan_delays().get(&id)?.duration()),
            PlanNode::Milestone(_) | PlanNode::TaskGroup(_) => None,
        };
        Some(Self {
            node,
            new: false,
            parent: model::parent_of(plan, node),
            dependencies: model::dependencies(plan, node),
            dependents: model::dependents(plan, node),
            duration,
            moved: (0, 0),
            pick: None,
        })
    }

    /// The draft of an item that is not in the plan yet. At the top level it starts from the
    /// plan's default start, if it has one.
    pub fn new(plan: &Plan, node: PlanNode, parent: Parent) -> Self {
        let start = plan.scheduler_data().default_start();
        let duration = match node {
            PlanNode::Task(_) | PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => Some(NEW_DAYS),
            PlanNode::Milestone(_) | PlanNode::TaskGroup(_) => None,
        };
        Self {
            node,
            new: true,
            parent,
            dependencies: start
                .filter(|_| parent == Parent::Plan)
                .into_iter()
                .collect(),
            dependents: Vec::new(),
            duration,
            moved: (0, 0),
            pick: None,
        }
    }

    /// The draft for the item whose pane opens: one being added to `new`, or one in the plan
    pub fn open(sync: Sync, plan: ShortId, node: PlanNode, new: Option<Parent>) -> Option<Self> {
        sync.tenant.with_untracked(|t| {
            let plan = t.as_ref()?.plans().get(&plan)?;
            match new {
                Some(parent) => Some(Self::new(plan, node, parent)),
                None => Self::of(plan, node),
            }
        })
    }

    /// Links the item to another, or takes the link away if it is there: the other is waited for
    /// (`Pick::Before`) or waits (`Pick::After`). Nothing changes if it would make a loop.
    pub fn toggle(&mut self, plan: &Plan, side: Pick, other: PlanNode) -> Result<(), String> {
        let mut next = self.clone();
        let list = match side {
            Pick::After => &mut next.dependents,
            _ => &mut next.dependencies,
        };
        match list.iter().position(|n| *n == other) {
            Some(at) => drop(list.remove(at)),
            None => list.push(other),
        }
        next.checked(plan)?;
        *self = next;
        Ok(())
    }

    /// Links the item to another if it is not already, see `toggle`
    pub fn link(&mut self, plan: &Plan, side: Pick, other: PlanNode) -> Result<(), String> {
        let linked = match side {
            Pick::After => self.dependents.contains(&other),
            _ => self.dependencies.contains(&other),
        };
        match linked {
            true => Ok(()),
            false => self.toggle(plan, side, other),
        }
    }

    /// Puts the item in the group dropped on, the group of an item dropped on, or at the top
    /// level for nothing
    pub fn place(&mut self, plan: &Plan, over: Option<PlanNode>) -> Result<(), String> {
        let parent = match over {
            Some(PlanNode::TaskGroup(id)) => Parent::Node(id),
            Some(other) => model::parent_of(plan, other),
            None => Parent::Plan,
        };
        let mut next = self.clone();
        next.parent = parent;
        next.checked(plan)?;
        *self = next;
        Ok(())
    }

    /// Whether the plan would take the draft, which it does not if it makes a loop
    fn checked(&self, plan: &Plan) -> Result<(), String> {
        if self.dependencies.contains(&self.node) || self.dependents.contains(&self.node) {
            return Err("An item cannot wait for itself".to_string());
        }
        let mut plan = plan.clone();
        let loops = |_| "That would make a loop of items waiting for each other".to_string();
        for update in self.updates(&plan) {
            plan.apply_update(update).map_err(loops)?;
        }
        Ok(())
    }

    /// The days a task really ran once they are moved as the draft says, never ending before
    /// they start
    pub fn dates(&self, state: PersistentState) -> PersistentState {
        let by = |day: NaiveDate, days: i32| day.checked_add_signed(Duration::days(days.into()));
        let start = state.actual_start_date.and_then(|d| by(d, self.moved.0));
        let end = state.actual_end_date.and_then(|d| by(d, self.moved.1));
        PersistentState {
            actual_start_date: start,
            actual_end_date: end.map(|e| start.map_or(e, |s| e.max(s))),
            ..state
        }
    }

    /// The update that adds the item, for one that is new
    pub fn adding(&self, name: String) -> PlanUpdate {
        let (id, parent) = (self.node.id(), self.parent);
        match self.node {
            PlanNode::Task(_) => PlanUpdate::Task(Crud::Add(NewTask { id, name, parent })),
            PlanNode::Milestone(_) => {
                PlanUpdate::Milestone(Crud::Add(NewMilestone { id, name, parent }))
            }
            PlanNode::TaskGroup(_) => {
                PlanUpdate::TaskGroup(Crud::Add(NewTaskGroup { id, name, parent }))
            }
            PlanNode::PlanBuffer(_) => PlanUpdate::PlanBuffer(Crud::Add(NewPlanBuffer {
                id,
                parent,
                duration: self.duration.unwrap_or(NEW_DAYS),
            })),
            PlanNode::PlanDelay(_) => PlanUpdate::PlanDelay(Crud::Add(NewPlanDelay {
                id,
                name: Some(name).filter(|n| !n.is_empty()),
                parent,
                duration: self.duration.unwrap_or(NEW_DAYS),
            })),
        }
    }

    /// The updates that make the links and the place of the item in the plan what the draft
    /// says, adding it first if it is new. Its other fields are the form's to save.
    fn updates(&self, plan: &Plan) -> Vec<PlanUpdate> {
        let mut updates = Vec::new();
        if self.new {
            updates.push(self.adding(String::new()));
        }
        let parent = Some(self.parent).filter(|_| !self.new);
        let dependencies = Some(self.dependencies.clone());
        updates.push(PlanUpdate::rewire(self.node, parent, dependencies));
        updates.extend(self.dependent_updates(plan, self.node));
        updates
    }

    /// The updates to what waits for the item, or no longer does, once the item is `node` in
    /// the plan
    pub fn dependent_updates(&self, plan: &Plan, node: PlanNode) -> Vec<PlanUpdate> {
        let before = match self.new {
            true => Vec::new(),
            false => model::dependents(plan, self.node),
        };
        let changed = |other: &&PlanNode| before.contains(other) != self.dependents.contains(other);
        let all = before.iter().chain(&self.dependents).filter(changed);
        let mut seen: Vec<PlanNode> = Vec::new();
        let mut updates = Vec::new();
        for other in all {
            if seen.contains(other) {
                continue;
            }
            seen.push(*other);
            let mut theirs = model::dependencies(plan, *other);
            theirs.retain(|d| *d != self.node && *d != node);
            if self.dependents.contains(other) {
                theirs.push(node);
            }
            updates.push(PlanUpdate::set_dependencies(*other, theirs));
        }
        updates
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;
    use plinko::{common::short_id::ShortId, planner::plan::NewPlan};
    use uuid::Uuid;

    /// A plan of three tasks where the second waits for the first, and a group
    fn plan() -> (Plan, [PlanNode; 3], Uuid) {
        let mut plan = Plan::from(NewPlan {
            id: ShortId::new(),
            name: "P".to_string(),
        });
        let tasks = [1u128, 2, 3].map(|n| PlanNode::Task(Uuid::from_u128(n)));
        for task in tasks {
            let draft = Draft::new(&plan, task, Parent::Plan);
            plan.apply_update(draft.adding("T".to_string())).unwrap();
        }
        plan.apply_update(PlanUpdate::set_dependencies(tasks[1], vec![tasks[0]]))
            .unwrap();
        let group = Uuid::from_u128(9);
        let adding = Draft::new(&plan, PlanNode::TaskGroup(group), Parent::Plan);
        plan.apply_update(adding.adding("G".to_string())).unwrap();
        (plan, tasks, group)
    }

    #[test]
    fn a_click_links_and_another_takes_the_link_away() {
        let (plan, [a, b, c], _) = plan();
        let mut draft = Draft::of(&plan, c).unwrap();
        draft.toggle(&plan, Pick::Before, b).unwrap();
        draft.toggle(&plan, Pick::After, a).unwrap_err();
        assert_eq!(
            (&draft.dependencies, &draft.dependents),
            (&vec![b], &vec![])
        );
        draft.toggle(&plan, Pick::Before, b).unwrap();
        assert!(draft.dependencies.is_empty());
        // dropping a link that is there leaves it
        let mut second = Draft::of(&plan, b).unwrap();
        second.link(&plan, Pick::Before, a).unwrap();
        assert_eq!(second.dependencies, vec![a]);
        assert!(second.toggle(&plan, Pick::Before, b).is_err(), "itself");
    }

    #[test]
    fn a_new_item_is_checked_like_one_in_the_plan() {
        let (plan, [a, b, _], group) = plan();
        let node = PlanNode::Task(Uuid::from_u128(7));
        let mut draft = Draft::new(&plan, node, Parent::Plan);
        assert!(draft.new && draft.dependencies.is_empty());
        draft.toggle(&plan, Pick::Before, b).unwrap();
        // a waits for the new item, which waits for b, which waits for a
        assert!(draft.toggle(&plan, Pick::After, a).is_err());
        assert!(draft.dependents.is_empty());
        draft
            .place(&plan, Some(PlanNode::TaskGroup(group)))
            .unwrap();
        assert_eq!(draft.parent, Parent::Node(group));
        draft.place(&plan, Some(a)).unwrap();
        assert_eq!(draft.parent, Parent::Plan);
    }

    #[test]
    fn only_what_changed_among_the_dependents_is_updated() {
        let (plan, [a, b, c], _) = plan();
        let mut draft = Draft::of(&plan, a).unwrap();
        assert_eq!(draft.dependents, vec![b]);
        assert!(draft.dependent_updates(&plan, a).is_empty());
        draft.toggle(&plan, Pick::After, c).unwrap();
        draft.toggle(&plan, Pick::After, b).unwrap();
        assert_eq!(draft.dependent_updates(&plan, a).len(), 2);
    }
}
// }}}
