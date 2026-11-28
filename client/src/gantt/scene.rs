//! Plans turned into lanes for the chart, plus the items the scheduler gave no dates.

// modules {{{
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Days, NaiveDate};
use plinko::{
    availability::day_value::DayValue,
    common::short_id::ShortId,
    planner::items::{dependency::PlanNode, parent::Parent, task::Assignee},
    planner::plan::Plan,
    scheduler::state::PlanVolatileState,
};
use uuid::Uuid;

use crate::{
    gantt::arrange::{Arrangement, Entry, Kind, Mark, Placed, arrange},
    model::{self, node_name},
};
// }}}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// Day 0: today, or the first day of anything earlier
    pub origin: NaiveDate,
    pub arrangement: Arrangement,
    pub unplaced: Vec<Unplaced>,
    /// The day offset after the last day anything is on
    pub last_day: f64,
    pub sections: Vec<Section>,
    /// Filled only when plans are boxed
    pub owners: HashMap<Uuid, ShortId>,
    /// The first copy, as split views can place an item more than once
    index: HashMap<Mark, usize>,
}

// Scene supporting types {{{
#[derive(Clone, Debug, PartialEq)]
pub struct Unplaced {
    pub node: PlanNode,
    pub name: String,
    pub reason: String,
}

/// A person's band: their name and pies lane, an optional lane of work on plans not shown, then their items
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub who: Uuid,
    pub name: String,
    pub lane: usize,
    pub lanes: usize,
    pub from: NaiveDate,
    /// (booked, capacity) for each day from `from`
    pub days: Vec<(u32, u8)>,
    /// The lane and runs of work on plans not shown; runs end on the day after their last
    pub other: Option<(usize, Vec<(NaiveDate, NaiveDate)>)>,
}

#[derive(Clone, Copy)]
pub struct Shown<'a> {
    pub id: ShortId,
    pub plan: &'a Plan,
    pub state: Option<&'a PlanVolatileState>,
}

pub struct Options<'a> {
    pub boxed: bool,
    pub only: Option<Uuid>,
    pub split: Option<Vec<(Uuid, String)>>,
    /// (booked, capacity) for each day from the first to the last, inclusive
    pub load: &'a dyn Fn(Uuid, NaiveDate, NaiveDate) -> Vec<(u32, u8)>,
}

/// Days past the last item that the pies cover
const LOAD_AHEAD: u64 = 120;

struct Context<'a> {
    plan: &'a Plan,
    state: Option<&'a PlanVolatileState>,
    collapsed: &'a HashSet<Uuid>,
    targets: &'a HashSet<PlanNode>,
    children: &'a HashMap<Parent, Vec<PlanNode>>,
}
// }}}

// impl Scene {{{
impl Scene {
    pub fn empty(origin: NaiveDate) -> Self {
        Self {
            origin,
            arrangement: Arrangement::default(),
            unplaced: Vec::new(),
            last_day: 0.0,
            sections: Vec::new(),
            owners: HashMap::new(),
            index: HashMap::new(),
        }
    }

    pub fn is_plan(&self, mark: Mark) -> bool {
        self.owners
            .get(&mark.id())
            .is_some_and(|plan| plan_mark(*plan) == mark.id())
    }

    pub fn placed(&self, mark: Mark) -> Option<&Placed> {
        self.arrangement.placed.get(*self.index.get(&mark)?)
    }
}
// }}}

// kind_of {{{
fn kind_of(node: PlanNode) -> Kind {
    match node {
        PlanNode::Task(_) => Kind::Task,
        PlanNode::Milestone(_) => Kind::Milestone,
        PlanNode::PlanBuffer(_) | PlanNode::PlanDelay(_) => Kind::Buffer,
        PlanNode::TaskGroup(_) => Kind::Group,
    }
}
// }}}

// build {{{
#[cfg(test)]
pub fn build(
    plan: &Plan,
    state: Option<&PlanVolatileState>,
    collapsed: &HashSet<Uuid>,
    today: NaiveDate,
) -> Scene {
    let shown = Shown {
        id: ShortId::default(),
        plan,
        state,
    };
    let options = Options {
        boxed: false,
        only: None,
        split: None,
        load: &|_, _, _| Vec::new(),
    };
    compose(&[shown], &options, collapsed, today)
}
// }}}

// compose {{{
pub fn compose(
    plans: &[Shown],
    options: &Options,
    collapsed: &HashSet<Uuid>,
    today: NaiveDate,
) -> Scene {
    let entries_for = |shown: &Shown, who: Option<Uuid>| -> Vec<Entry> {
        let keep = who.map(|who| related(shown.plan, shown.state, who));
        if keep.as_ref().is_some_and(HashSet::is_empty) {
            return Vec::new();
        }
        let entries = plan_entries(shown, keep.as_ref(), collapsed);
        if options.boxed {
            boxed(shown, entries, collapsed).into_iter().collect()
        } else {
            entries
        }
    };
    let mut unplaced = Vec::new();
    if !options.boxed {
        for shown in plans {
            let keep = options
                .only
                .map(|who| related(shown.plan, shown.state, who));
            unplaced.extend(unplaced_of(shown, keep.as_ref()));
        }
    }
    unplaced.sort_by(|a, b| a.name.cmp(&b.name).then(a.node.id().cmp(&b.node.id())));

    let (arrangement, drafts) = match &options.split {
        None => {
            let entries: Vec<Entry> = plans
                .iter()
                .flat_map(|shown| entries_for(shown, options.only))
                .collect();
            (arrange(&entries), Vec::new())
        }
        Some(people) => {
            let drafts: Vec<(Uuid, String, Arrangement)> = people
                .iter()
                .map(|(who, name)| {
                    let entries: Vec<Entry> = plans
                        .iter()
                        .flat_map(|shown| entries_for(shown, Some(*who)))
                        .collect();
                    (*who, name.clone(), arrange(&entries))
                })
                .collect();
            (Arrangement::default(), drafts)
        }
    };
    let spans = arrangement
        .placed
        .iter()
        .chain(drafts.iter().flat_map(|d| d.2.placed.iter()))
        .map(|p| p.span);
    let first = spans.clone().map(|s| s.0).min();
    let origin = first.map_or(today, |first| first.min(today));
    let last = spans.map(|s| s.1).max().unwrap_or(origin);

    let (arrangement, sections) = if options.split.is_some() {
        lay_out(plans, options, drafts, origin, last.max(today))
    } else {
        (arrangement, Vec::new())
    };
    let mut index = HashMap::new();
    for (i, p) in arrangement.placed.iter().enumerate() {
        index.entry(p.mark).or_insert(i);
    }
    let mut owners = HashMap::new();
    if options.boxed {
        for shown in plans {
            owners.insert(plan_mark(shown.id), shown.id);
            for node in model::nodes(shown.plan) {
                owners.insert(node.id(), shown.id);
            }
        }
    }
    Scene {
        origin,
        last_day: (last - origin).num_days() as f64,
        arrangement,
        unplaced,
        sections,
        owners,
        index,
    }
}
// }}}

// lay_out {{{
fn lay_out(
    plans: &[Shown],
    options: &Options,
    drafts: Vec<(Uuid, String, Arrangement)>,
    origin: NaiveDate,
    last: NaiveDate,
) -> (Arrangement, Vec<Section>) {
    let to = last + Days::new(LOAD_AHEAD);
    let mut placed = Vec::new();
    let mut sections = Vec::new();
    let mut lane = 0;
    for (who, name, inner) in drafts {
        // an empty lane between people
        if !sections.is_empty() {
            lane += 1;
        }
        let days = (options.load)(who, origin, to);
        // hours booked beyond the plans shown are on plans not shown
        let mut shown_hours: BTreeMap<NaiveDate, u32> = BTreeMap::new();
        for shown in plans {
            for task in shown.state.into_iter().flat_map(|s| s.tasks.values()) {
                for (day, hours) in task
                    .assignments
                    .get(&Assignee::User(who))
                    .into_iter()
                    .flatten()
                {
                    *shown_hours.entry(*day).or_default() += u32::from(hours.get());
                }
            }
        }
        let other_hours: Vec<u32> = days
            .iter()
            .enumerate()
            .map(|(i, (booked, _))| {
                let day = origin + Days::new(i as u64);
                booked.saturating_sub(shown_hours.get(&day).copied().unwrap_or(0))
            })
            .collect();
        let runs = runs_of(&other_hours, origin);
        let other_lane = (!runs.is_empty()).then_some(lane + 1);
        let first_inner = lane + 1 + usize::from(other_lane.is_some());
        let lanes = first_inner - lane + inner.lanes;
        placed.extend(inner.placed.into_iter().map(|mut p| {
            p.lane += first_inner;
            p
        }));
        sections.push(Section {
            who,
            name,
            lane,
            lanes,
            from: origin,
            days,
            other: other_lane.map(|l| (l, runs)),
        });
        lane += lanes;
    }
    (
        Arrangement {
            placed,
            lanes: lane,
        },
        sections,
    )
}

fn runs_of(hours: &[u32], from: NaiveDate) -> Vec<(NaiveDate, NaiveDate)> {
    let mut runs = Vec::new();
    let mut start = None;
    for (i, h) in hours.iter().chain(std::iter::once(&0)).enumerate() {
        let day = from + Days::new(i as u64);
        match (start, *h > 0) {
            (None, true) => start = Some(day),
            (Some(s), false) => {
                runs.push((s, day));
                start = None;
            }
            _ => {}
        }
    }
    runs
}
// }}}

// related {{{
/// The tasks someone is booked on, the milestones directly before or after them, and every
/// group above those
pub fn related(plan: &Plan, state: Option<&PlanVolatileState>, who: Uuid) -> HashSet<PlanNode> {
    let Some(state) = state else {
        return HashSet::new();
    };
    let tasks: HashSet<PlanNode> = state
        .tasks
        .iter()
        .filter(|(id, t)| {
            t.assignments.contains_key(&Assignee::User(who)) && plan.tasks().contains_key(id)
        })
        .map(|(id, _)| PlanNode::Task(*id))
        .collect();
    let mut keep = tasks.clone();
    for (id, milestone) in plan.milestones() {
        let node = PlanNode::Milestone(*id);
        let after = milestone.dependencies().iter().any(|d| tasks.contains(d));
        let before = tasks
            .iter()
            .any(|t| model::dependencies(plan, *t).contains(&node));
        if after || before {
            keep.insert(node);
        }
    }
    for node in keep.clone() {
        let mut at = model::parent_of(plan, node);
        while let Parent::Node(group) = at {
            if !keep.insert(PlanNode::TaskGroup(group)) {
                break;
            }
            at = model::parent_of(plan, PlanNode::TaskGroup(group));
        }
    }
    keep
}
// }}}

// plan_mark {{{
/// A fake id for a plan's box that no item can have
pub fn plan_mark(plan: ShortId) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xb0;
    for (i, b) in plan.to_string().bytes().take(15).enumerate() {
        bytes[i + 1] = b;
    }
    Uuid::from_bytes(bytes)
}
// }}}

// plan_entries {{{
fn plan_entries(
    shown: &Shown,
    keep: Option<&HashSet<PlanNode>>,
    collapsed: &HashSet<Uuid>,
) -> Vec<Entry> {
    let (plan, state) = (shown.plan, shown.state);
    let targets: HashSet<PlanNode> = plan.targets().values().map(|t| t.node()).collect();
    let mut children: HashMap<Parent, Vec<PlanNode>> = HashMap::new();
    for node in model::nodes(plan) {
        if keep.is_some_and(|k| !k.contains(&node)) || model::span_of(state, node).is_none() {
            continue;
        }
        children
            .entry(model::parent_of(plan, node))
            .or_default()
            .push(node);
    }
    let context = Context {
        plan,
        state,
        collapsed,
        targets: &targets,
        children: &children,
    };
    context.entries(Parent::Plan)
}

fn unplaced_of(shown: &Shown, keep: Option<&HashSet<PlanNode>>) -> Vec<Unplaced> {
    let (plan, state) = (shown.plan, shown.state);
    model::nodes(plan)
        .into_iter()
        .filter(|node| keep.is_none_or(|k| k.contains(node)))
        .filter(|node| model::span_of(state, *node).is_none())
        .map(|node| {
            let reason = state
                .and_then(|s| s.unscheduled.get(&node))
                .map_or("Not scheduled yet", |r| model::reason(*r));
            Unplaced {
                node,
                name: node_name(plan, node),
                reason: reason.to_string(),
            }
        })
        .collect()
}

fn boxed(shown: &Shown, members: Vec<Entry>, collapsed: &HashSet<Uuid>) -> Option<Entry> {
    let start = members.iter().map(|e| e.span.0).min()?;
    let end = members.iter().map(|e| e.span.1).max()?;
    let mark = plan_mark(shown.id);
    Some(Entry {
        node: PlanNode::TaskGroup(mark),
        name: shown.plan.name().to_string(),
        kind: Kind::Group,
        span: (start, end),
        look: model::Look::NotStarted,
        violated: false,
        pinned: false,
        target: false,
        members,
        collapsed: collapsed.contains(&mark),
    })
}
// }}}

// impl Context {{{
impl Context<'_> {
    fn entries(&self, parent: Parent) -> Vec<Entry> {
        let nodes = self.children.get(&parent).into_iter().flatten().copied();
        nodes
            .filter_map(|node| {
                let span = model::span_of(self.state, node)?;
                let members = match node {
                    PlanNode::TaskGroup(id) => self.entries(Parent::Node(id)),
                    _ => Vec::new(),
                };
                let (name, span) = match node {
                    PlanNode::PlanBuffer(id) => self.buffer(id, span),
                    // an unnamed delay's shape tells it from a buffer, so it shows only its days
                    PlanNode::PlanDelay(id) => match self.plan.plan_delays().get(&id) {
                        Some(delay) if delay.name().is_none() => {
                            (format!("{}d", delay.duration()), span)
                        }
                        _ => (node_name(self.plan, node), span),
                    },
                    _ => (node_name(self.plan, node), span),
                };
                Some(Entry {
                    node,
                    name,
                    kind: kind_of(node),
                    span,
                    look: model::look_of(self.plan, self.state, node),
                    violated: self.state.is_some_and(|s| s.constraint_violations.contains_key(&node)),
                    pinned: matches!(node, PlanNode::PlanBuffer(id)
                        if self.plan.plan_buffers().get(&id).is_some_and(|b| b.set_start_day().is_some())),
                    target: self.targets.contains(&node),
                    members,
                    collapsed: matches!(node, PlanNode::TaskGroup(id) if self.collapsed.contains(&id)),
                })
            })
            .collect()
    }
}

impl Context<'_> {
    /// Its label is the working days left, and the days given when they differ. A buffer with no
    /// days left still spans a day, to be clickable
    fn buffer(&self, id: Uuid, span: (NaiveDate, NaiveDate)) -> (String, (NaiveDate, NaiveDate)) {
        let availability = self.plan.default_availability();
        let now = span
            .0
            .iter_days()
            .take_while(|d| *d < span.1)
            .filter(|d| availability.value_on(*d).is_working())
            .count();
        let given = self
            .plan
            .plan_buffers()
            .get(&id)
            .map_or(0, |b| *b.duration() as usize);
        let name = if now == given {
            format!("{now}d")
        } else {
            format!("{now}d ({given}d)")
        };
        (name, (span.0, span.1.max(span.0 + chrono::Days::new(1))))
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use plinko::{
        common::{crud::Crud, dates::DateRange, short_id::ShortId},
        planner::items::{
            dependency::PlanNode,
            task::{self, NewTask, TaskPatch},
        },
        planner::{plan::NewPlan, plan_update::PlanUpdate},
        scheduler::state::UnscheduledReason,
    };

    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    fn scheduled(state: &mut PlanVolatileState, id: Uuid, from: u32, to: u32) {
        state.tasks.insert(
            id,
            task::VolatileState {
                date_range: DateRange::new(d(from), d(to)).unwrap(),
                assignments: Default::default(),
                workers: Default::default(),
            },
        );
    }

    #[test]
    fn items_get_lanes_leaves_are_marked_and_the_unscheduled_are_listed() {
        let mut plan = Plan::from(NewPlan {
            id: ShortId::new(),
            name: "P".to_string(),
        });
        let ids: Vec<Uuid> = (1..=3).map(Uuid::from_u128).collect();
        for (i, id) in ids.iter().enumerate() {
            let new = NewTask {
                id: *id,
                name: format!("T{i}"),
                parent: Parent::Plan,
            };
            plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
        }
        // T1 waits for T0
        let patch = TaskPatch {
            id: ids[1],
            name: None,
            parent: None,
            duration: None,
            time_constraint: None,
            dependencies: Some(vec![PlanNode::Task(ids[0])]),
            allocations: None,
            affinities: None,
            actual_workers: None,
            persistent_state: None,
        };
        plan.apply_update(PlanUpdate::Task(Crud::Update(patch)))
            .unwrap();
        let mut state = PlanVolatileState::default();
        scheduled(&mut state, ids[0], 3, 5);
        scheduled(&mut state, ids[1], 6, 8);
        state
            .unscheduled
            .insert(PlanNode::Task(ids[2]), UnscheduledReason::NoEligibleWorkers);

        let scene = build(&plan, Some(&state), &HashSet::new(), d(1));
        assert_eq!(scene.origin, d(1));
        let get = |name: &str| {
            scene
                .arrangement
                .placed
                .iter()
                .find(|p| p.name == name)
                .unwrap()
        };
        // T0 and T1 in the first lane with a day between them
        assert_eq!(scene.arrangement.placed.len(), 2);
        assert_eq!((get("T0").lane, get("T1").lane), (0, 0));
        assert_eq!(scene.last_day, 7.0);
        assert_eq!(scene.unplaced.len(), 1);
        assert_eq!(scene.unplaced[0].name, "T2");
        assert!(scene.unplaced[0].reason.contains("eligible"));
    }

    #[test]
    fn a_person_gets_their_tasks_the_milestones_next_to_them_and_their_groups() {
        use plinko::{
            availability::workload::Workload,
            planner::items::{
                milestone::{MilestonePatch, NewMilestone},
                task_group::NewTaskGroup,
            },
        };
        let mut plan = Plan::from(NewPlan {
            id: ShortId::new(),
            name: "P".to_string(),
        });
        let id = Uuid::from_u128;
        let group = NewTaskGroup {
            id: id(10),
            name: "G".to_string(),
            parent: Parent::Plan,
        };
        plan.apply_update(PlanUpdate::TaskGroup(Crud::Add(group)))
            .unwrap();
        for (n, parent) in [(1, Parent::Node(id(10))), (2, Parent::Plan)] {
            let new = NewTask {
                id: id(n),
                name: format!("T{n}"),
                parent,
            };
            plan.apply_update(PlanUpdate::Task(Crud::Add(new))).unwrap();
        }
        for (n, after) in [(20, 1), (21, 2)] {
            let new = NewMilestone {
                id: id(n),
                name: format!("M{n}"),
                parent: Parent::Plan,
            };
            plan.apply_update(PlanUpdate::Milestone(Crud::Add(new)))
                .unwrap();
            let patch = MilestonePatch {
                id: id(n),
                name: None,
                parent: None,
                time_constraint: None,
                dependencies: Some(vec![PlanNode::Task(id(after))]),
                persistent_state: None,
            };
            plan.apply_update(PlanUpdate::Milestone(Crud::Update(patch)))
                .unwrap();
        }
        let mut state = PlanVolatileState::default();
        scheduled(&mut state, id(1), 3, 6);
        scheduled(&mut state, id(2), 3, 6);
        state.task_groups.insert(
            id(10),
            plinko::planner::items::task_group::VolatileState {
                date_range: DateRange::new(d(3), d(6)).unwrap(),
            },
        );
        let who = id(99);
        let hours: BTreeMap<NaiveDate, Workload> = (3..6)
            .map(|d_| (d(d_), Workload::new(8).unwrap()))
            .collect();
        state
            .tasks
            .get_mut(&id(1))
            .unwrap()
            .assignments
            .insert(Assignee::User(who), hours);

        let mine = related(&plan, Some(&state), who);
        let expected: HashSet<PlanNode> = [
            PlanNode::Task(id(1)),
            PlanNode::Milestone(id(20)),
            PlanNode::TaskGroup(id(10)),
        ]
        .into_iter()
        .collect();
        assert_eq!(mine, expected);

        // split, with a day of work on a plan not shown: the band, the lane of that work, then
        // the group with the task in it; the other person has only their band
        let shown = [Shown {
            id: ShortId::new(),
            plan: &plan,
            state: Some(&state),
        }];
        let load = |person: Uuid, from: NaiveDate, to: NaiveDate| -> Vec<(u32, u8)> {
            from.iter_days()
                .take_while(|day| *day <= to)
                .map(|day| {
                    let task = (d(3)..d(6)).contains(&day);
                    let other = day == d(8);
                    let booked = if person == who && (task || other) {
                        8
                    } else {
                        0
                    };
                    (booked, 8)
                })
                .collect()
        };
        let options = Options {
            boxed: false,
            only: None,
            split: Some(vec![(who, "Me".to_string()), (id(98), "You".to_string())]),
            load: &load,
        };
        let scene = compose(&shown, &options, &HashSet::new(), d(1));
        let lanes: Vec<(usize, usize)> = scene.sections.iter().map(|s| (s.lane, s.lanes)).collect();
        assert_eq!(lanes, vec![(0, 4), (5, 1)]);
        let other = scene.sections[0].other.as_ref().unwrap();
        assert_eq!(other, &(1, vec![(d(8), d(9))]));
        assert_eq!(scene.placed(PlanNode::TaskGroup(id(10))).unwrap().lane, 2);
        assert_eq!(scene.placed(PlanNode::Task(id(1))).unwrap().lane, 3);
        assert!(scene.placed(PlanNode::Task(id(2))).is_none());
    }
}
// }}}
