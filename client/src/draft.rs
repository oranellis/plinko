//! An item's unsaved edits, applied to the plan only on save so they can be discarded whole.

// modules {{{
use std::collections::HashSet;

use chrono::{Duration, NaiveDate};
use leptos::prelude::*;
use plinko::{
    common::{
        crud::Crud,
        dates::{NodeConstraint, NodeEnd},
        short_id::ShortId,
    },
    planner::items::{
        dependency::PlanNode,
        milestone::NewMilestone,
        parent::Parent,
        plan_buffer::NewPlanBuffer,
        plan_delay::NewPlanDelay,
        task::{NewTask, PersistentState, Worker},
        task_group::NewTaskGroup,
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
};
use uuid::Uuid;

use crate::{
    forms,
    gantt::shift::{Held, Pin},
    model,
    sync::Sync,
};
// }}}

// constants {{{
pub const NEW_DAYS: u32 = 3;
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub node: PlanNode,
    /// Not in the plan yet, so saving adds it
    pub new: bool,
    /// Empty for a buffer, and for a delay shown by its days
    pub name: String,
    /// A milestone has no end to choose
    pub constraint: NodeConstraint,
    /// The last pin chosen (from the plan or moved since), which holding it again goes back to
    pub chosen: Option<Pin>,
    /// Workers and their hours a day
    pub allocations: Vec<(Worker, u8)>,
    pub affinities: HashSet<Uuid>,
    pub parent: Parent,
    pub dependencies: Vec<PlanNode>,
    pub dependents: Vec<PlanNode>,
    /// `None` for kinds without days
    pub duration: Option<u32>,
    /// Days the actual start and actual end are shifted by
    pub moved: (i32, i32),
    /// The lit card handle, which chart clicks then answer to
    pub pick: Option<Pick>,
    /// While its day is picked, where it would go, if not where it is
    pub aim: Option<NaiveDate>,
}

// Draft supporting types {{{
/// What a chart click does while a card handle is lit
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pick {
    /// The item waits for what is clicked
    Before,
    /// What is clicked waits for the item
    After,
    /// The item goes in the group clicked, or the group of the item clicked
    Group,
    /// The day the item is held to moves to the day clicked
    Day,
}

// }}}

// impl Draft {{{
impl Draft {
    pub fn of(plan: &Plan, node: PlanNode) -> Option<Self> {
        let duration = match node {
            PlanNode::Task(id) => Some(plan.tasks().get(&id)?.duration()),
            PlanNode::PlanBuffer(id) => Some(*plan.plan_buffers().get(&id)?.duration()),
            PlanNode::PlanDelay(id) => Some(*plan.plan_delays().get(&id)?.duration()),
            PlanNode::Milestone(_) | PlanNode::TaskGroup(_) => None,
        };
        let (name, constraint) = match node {
            PlanNode::Task(id) => {
                let task = plan.tasks().get(&id)?;
                (task.name().to_string(), task.time_constraint())
            }
            PlanNode::Milestone(id) => {
                let milestone = plan.milestones().get(&id)?;
                let constraint = NodeConstraint {
                    constraint: milestone.time_constraint(),
                    ..NodeConstraint::default()
                };
                (milestone.name().to_string(), constraint)
            }
            PlanNode::TaskGroup(id) => {
                let group = plan.task_groups().get(&id)?;
                (group.name().to_string(), group.time_constraint())
            }
            PlanNode::PlanDelay(id) => {
                let name = plan.plan_delays().get(&id)?.name().unwrap_or_default();
                (name.to_string(), NodeConstraint::default())
            }
            PlanNode::PlanBuffer(_) => (String::new(), NodeConstraint::default()),
        };
        let task = match node {
            PlanNode::Task(id) => plan.tasks().get(&id),
            _ => None,
        };
        let mut allocations: Vec<(Worker, u8)> = task
            .map(|t| t.allocations().iter().map(|(w, h)| (*w, h.get())).collect())
            .unwrap_or_default();
        allocations.sort_by_key(|(w, _)| forms::worker_value(*w));
        Some(Self {
            node,
            new: false,
            name,
            constraint,
            chosen: Pin::of(
                constraint.constraint,
                constraint.end == NodeEnd::End || matches!(node, PlanNode::Milestone(_)),
            ),
            allocations,
            affinities: task.map(|t| t.affinities().clone()).unwrap_or_default(),
            parent: model::parent_of(plan, node),
            dependencies: model::dependencies(plan, node),
            dependents: model::dependents(plan, node),
            duration,
            moved: (0, 0),
            pick: None,
            aim: None,
        })
    }

    /// At the top level it starts from the plan's default start, if it has one
    pub fn new(plan: &Plan, node: PlanNode, parent: Parent) -> Self {
        let start = plan.scheduler_data().default_start();
        let duration = match node {
            PlanNode::Task(_) | PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => Some(NEW_DAYS),
            PlanNode::Milestone(_) | PlanNode::TaskGroup(_) => None,
        };
        let name = match node {
            PlanNode::Task(_) => "New task",
            PlanNode::Milestone(_) => "New milestone",
            PlanNode::TaskGroup(_) => "New group",
            PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => "",
        };
        Self {
            node,
            new: true,
            name: name.to_string(),
            constraint: NodeConstraint::default(),
            chosen: None,
            allocations: Vec::new(),
            affinities: HashSet::new(),
            parent,
            dependencies: start
                .filter(|_| parent == Parent::Plan)
                .into_iter()
                .collect(),
            dependents: Vec::new(),
            duration,
            moved: (0, 0),
            pick: None,
            aim: None,
        }
    }

    /// `new` is the parent of an item being added; `None` opens one in the plan
    pub fn open(sync: Sync, plan: ShortId, node: PlanNode, new: Option<Parent>) -> Option<Self> {
        // whoever adds a task is allocated half a day by default
        let me = sync.me.get_untracked().map(|m| (Worker::User(m.id), 4));
        sync.tenant.with_untracked(|t| {
            let plan = t.as_ref()?.plans().get(&plan)?;
            match new {
                Some(parent) => {
                    let mut draft = Self::new(plan, node, parent);
                    draft
                        .allocations
                        .extend(me.filter(|_| matches!(node, PlanNode::Task(_))));
                    Some(draft)
                }
                None => Self::of(plan, node),
            }
        })
    }

    /// Toggles a link: the other is waited for (`Pick::Before`) or waits (`Pick::After`).
    /// Refused if it would make a loop.
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

    /// Like `toggle`, but never unlinks
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

    /// Into the group dropped on, the group of the item dropped on, or the top level for nothing
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

    /// Refuses a draft that makes a loop
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

    /// A milestone is held by its end
    pub fn pin(&self) -> Option<Pin> {
        let end =
            self.constraint.end == NodeEnd::End || matches!(self.node, PlanNode::Milestone(_));
        Pin::of(self.constraint.constraint, end)
    }

    /// Cycles nothing held, first day held, last day held (a milestone: held or not). Until a
    /// day is chosen it uses the scheduled `first` and `last`; after that only the end changes.
    pub fn hold_next(&mut self, first: NaiveDate, last: NaiveDate) {
        let (start, end) = (
            Pin {
                day: first,
                held: Held::Earliest,
                end: false,
            },
            Pin {
                day: last,
                held: Held::Latest,
                end: true,
            },
        );
        let single = matches!(self.node, PlanNode::Milestone(_));
        let (start, end) = match self.chosen {
            Some(chosen) => (
                Pin {
                    end: false,
                    ..chosen
                },
                Pin {
                    end: true,
                    ..chosen
                },
            ),
            None => (start, end),
        };
        self.hold(match self.pin() {
            None if single => Some(end),
            None => Some(start),
            Some(pin) if !pin.end => Some(end),
            Some(_) => None,
        });
    }

    /// Also remembered in `chosen`
    pub fn hold_as(&mut self, held: Held) {
        if let Some(pin) = self.pin() {
            self.hold(Some(Pin { held, ..pin }));
            self.chosen = self.chosen.map(|chosen| Pin { held, ..chosen });
        }
    }

    /// Toggles the lit handle
    pub fn pick(&mut self, pick: Pick) {
        self.pick = Some(pick).filter(|p| self.pick != Some(*p));
        self.aim = None;
    }

    /// From the current aim, or from the pinned day if there is none
    pub fn aim_by(&mut self, days: i64) {
        if let Some(pin) = self.pin() {
            let from = self.aim.unwrap_or(pin.day);
            self.aim = from.checked_add_signed(Duration::days(days)).or(Some(from));
        }
    }

    /// Moves the pin to the aim and ends the picking
    pub fn aimed(&mut self) {
        if let Some(pin) = self.pin() {
            let day = self.aim.unwrap_or(pin.day);
            self.hold(Some(Pin { day, ..pin }));
            self.chosen = self.pin();
        }
        self.pick = None;
        self.aim = None;
    }

    /// `None` also ends any day picking
    pub fn hold(&mut self, pin: Option<Pin>) {
        if pin.is_none() {
            self.pick = self.pick.filter(|p| *p != Pick::Day);
        }
        self.constraint = match pin {
            Some(pin) => NodeConstraint {
                constraint: pin.constraint(),
                end: if pin.end {
                    NodeEnd::End
                } else {
                    NodeEnd::Start
                },
            },
            None => NodeConstraint::default(),
        };
    }

    /// The actual dates shifted by `moved`, never ending before they start
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

    pub fn adding(&self) -> PlanUpdate {
        let name = self.name.clone();
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

    /// Links and placement only, adding the item first if new; the form saves the other fields
    fn updates(&self, plan: &Plan) -> Vec<PlanUpdate> {
        let mut updates = Vec::new();
        if self.new {
            updates.push(self.adding());
        }
        let parent = Some(self.parent).filter(|_| !self.new);
        let dependencies = Some(self.dependencies.clone());
        updates.push(PlanUpdate::rewire(self.node, parent, dependencies));
        updates.extend(self.dependent_updates(plan, self.node));
        updates
    }

    /// Replays the changes `self` made to `was` (as opened) onto `now`, so concurrent edits by
    /// others survive: only changed fields are taken, and lists keep others' additions and removals
    pub fn onto(&self, was: &Draft, now: Draft) -> Draft {
        fn pick<T: PartialEq + Clone>(mine: &T, was: &T, now: T) -> T {
            if mine == was { now } else { mine.clone() }
        }
        fn merge<T: PartialEq + Clone>(mine: &[T], was: &[T], mut now: Vec<T>) -> Vec<T> {
            now.retain(|n| mine.contains(n) || !was.contains(n));
            let added = mine.iter().filter(|m| !was.contains(m) && !now.contains(m));
            let added: Vec<T> = added.cloned().collect();
            now.extend(added);
            now
        }
        let affinities = |d: &Draft| d.affinities.iter().copied().collect::<Vec<_>>();
        let merged = merge(&affinities(self), &affinities(was), affinities(&now));
        Draft {
            name: pick(&self.name, &was.name, now.name),
            constraint: pick(&self.constraint, &was.constraint, now.constraint),
            chosen: pick(&self.chosen, &was.chosen, now.chosen),
            allocations: pick(&self.allocations, &was.allocations, now.allocations),
            affinities: merged.into_iter().collect(),
            parent: pick(&self.parent, &was.parent, now.parent),
            dependencies: merge(&self.dependencies, &was.dependencies, now.dependencies),
            dependents: merge(&self.dependents, &was.dependents, now.dependents),
            duration: pick(&self.duration, &was.duration, now.duration),
            moved: self.moved,
            ..now
        }
    }

    /// Updates to items that start or stop waiting for it, once it is `node` in the plan
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

    /// A plan of three tasks where the second waits for the first, and a group
    fn plan() -> (Plan, [PlanNode; 3], Uuid) {
        let mut plan = Plan::from(NewPlan {
            id: ShortId::new(),
            name: "P".to_string(),
        });
        let tasks = [1u128, 2, 3].map(|n| PlanNode::Task(Uuid::from_u128(n)));
        for task in tasks {
            let draft = Draft::new(&plan, task, Parent::Plan);
            plan.apply_update(draft.adding()).unwrap();
        }
        plan.apply_update(PlanUpdate::set_dependencies(tasks[1], vec![tasks[0]]))
            .unwrap();
        let group = Uuid::from_u128(9);
        let adding = Draft::new(&plan, PlanNode::TaskGroup(group), Parent::Plan);
        plan.apply_update(adding.adding()).unwrap();
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

    #[test]
    fn saving_keeps_what_others_changed_meanwhile() {
        let (plan, [a, b, c], _) = plan();
        let was = Draft::of(&plan, c).unwrap();
        // here the name was changed and b was linked
        let mut mine = was.clone();
        mine.name = "Mine".to_string();
        mine.dependencies.push(b);
        // someone else gave it days and linked a
        let mut now = was.clone();
        now.duration = Some(9);
        now.dependencies.push(a);
        let saved = mine.onto(&was, now);
        assert_eq!(saved.name, "Mine");
        assert_eq!(saved.duration, Some(9));
        assert_eq!(saved.dependencies, vec![a, b]);
    }
}
// }}}
