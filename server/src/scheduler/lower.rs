// modules {{{
use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    common::dates::NodeEnd,
    multitenancy::tenant::Tenant,
    planner::{
        items::{
            dependency::PlanNode,
            milestone::{DerivedStatus, Milestone},
            parent::Parent,
            plan_buffer::PlanBuffer,
            plan_delay::PlanDelay,
            task::{Assignee, Status, Task, Worker},
            task_group::{EMPTY_DAYS, TaskGroup},
        },
        plan::Plan,
        plan_entity::{AbstractPlanNode, PlanItem},
    },
    scheduler::{
        arc::{Arc, ArcKind, Buffer, Progress, Slot, Until, Work},
        day::Day,
        network::{Lowered, Network, PlanInfo, Target, Vertex, VertexId},
        table::{RowId, RowKey, Table},
    },
    scheduler::{state::UnscheduledReason, status::milestone_statuses},
};
// }}}

// lower {{{
/// In plan then node id order so runs are deterministic
pub fn lower(tenant: &Tenant, table: &mut Table) -> Network {
    let mut net = Network::default();
    net.root = net.add_vertex(Vertex {
        pinned: Some(table.today),
        ..Vertex::default()
    });
    let mut plans: Vec<&Plan> = tenant.plans().values().collect();
    plans.sort_by_key(|p| *p.id());
    let mut joins = HashMap::new();
    for (index, plan) in plans.into_iter().enumerate() {
        let mut lowering = Lowering {
            tenant,
            plan,
            index: index as u32,
            table: &mut *table,
            net: &mut net,
            joins: &mut joins,
            statuses: milestone_statuses(plan),
            items: HashMap::new(),
            starts: HashMap::new(),
            ends: HashMap::new(),
            visiting: HashSet::new(),
        };
        lowering.plan();
    }
    net.link();
    net
}
// }}}

struct Lowering<'a> {
    tenant: &'a Tenant,
    plan: &'a Plan,
    index: u32,
    table: &'a mut Table,
    net: &'a mut Network,
    /// Keyed by group anchor and the vertices waited on together
    joins: &'a mut HashMap<(Option<VertexId>, Vec<VertexId>), VertexId>,
    statuses: HashMap<Uuid, DerivedStatus>,
    items: HashMap<PlanNode, Item>,
    starts: HashMap<PlanNode, VertexId>,
    ends: HashMap<PlanNode, VertexId>,
    /// So a dependency cycle ends instead of recursing for ever
    visiting: HashSet<PlanNode>,
}

#[derive(Clone)]
struct Item {
    parent: Parent,
    dependencies: Vec<PlanNode>,
    dropped: bool,
}

// impl Lowering {{{
impl Lowering<'_> {
    fn plan(&mut self) {
        let plan = self.plan;
        let lists = [
            items::<Task>(plan),
            items::<Milestone>(plan),
            items::<PlanBuffer>(plan),
            items::<PlanDelay>(plan),
            items::<TaskGroup>(plan),
        ];
        let mut nodes: Vec<PlanNode> = Vec::new();
        for (node, mut item) in lists.into_iter().flatten() {
            if let PlanNode::Task(id) = node {
                item.dropped = plan.tasks()[&id].persistent_state().status == Status::Dropped;
            }
            nodes.push(node);
            self.items.insert(node, item);
        }
        // everything but a dropped task ends on a vertex of its own, a dropped task where it starts
        for node in &nodes {
            if !self.items[node].dropped {
                let end = self.net.add_vertex(Vertex::default());
                self.ends.insert(*node, end);
            }
        }
        for node in &nodes {
            self.wire(*node);
        }
        for (id, group) in plan.task_groups() {
            if let Some(date) = group.time_constraint().bounds_on(NodeEnd::End).0 {
                self.end_not_before(*id, self.day(date).next());
            }
        }
        let ends = nodes.iter().filter(|n| !self.items[n].dropped);
        let mut all: Vec<VertexId> = ends.map(|n| self.ends[n]).collect();
        all.sort();
        let end = self.net.add_vertex(Vertex {
            after: all,
            ..Vertex::default()
        });
        let mut targets: Vec<Target> = plan
            .targets()
            .iter()
            .filter_map(|(id, target)| {
                Some(Target {
                    id: *id,
                    vertex: self.net.lowered.get(&(self.index, target.node()))?.end,
                    priority: i64::from(target.priority()),
                })
            })
            .collect();
        targets.sort_by_key(|t| t.id);
        self.net.plans.push(PlanInfo {
            id: *plan.id(),
            end,
            priority: i64::from(plan.scheduler_data().priority()),
            targets,
        });
    }

    fn day(&self, date: NaiveDate) -> Day {
        Day::of(date, self.table.origin)
    }

    fn now(&self) -> Day {
        self.table.today
    }

    /// `anchor` is the start of the enclosing group, none at the top level. Shared by everything
    /// that waits on the same; with nothing to wait on it is the group's or plan's start.
    fn join(&mut self, anchor: Option<VertexId>, mut after: Vec<VertexId>) -> VertexId {
        after.sort();
        after.dedup();
        match (anchor, after.as_slice()) {
            (None, []) => self.net.root,
            (Some(start), []) => start,
            (None, [one]) => *one,
            _ => {
                let key = (anchor, after);
                if let Some(join) = self.joins.get(&key) {
                    return *join;
                }
                let join = self.net.add_vertex(Vertex {
                    anchor,
                    after: key.1.clone(),
                    ..Vertex::default()
                });
                self.joins.insert(key, join);
                join
            }
        }
    }

    fn start_of(&mut self, node: PlanNode) -> VertexId {
        if let Some(start) = self.starts.get(&node) {
            return *start;
        }
        let Some(item) = self.items.get(&node).cloned() else {
            return self.net.root;
        };
        if !self.visiting.insert(node) {
            return self.net.root;
        }
        let anchor = match item.parent {
            Parent::Node(group) => Some(self.start_of(PlanNode::TaskGroup(group))),
            Parent::Plan => None,
        };
        let mut after = Vec::new();
        for on in &item.dependencies {
            if self.items.contains_key(on) {
                after.push(self.end_of(*on));
            }
        }
        let mut start = self.join(anchor, after);
        // a vertex of its own, as the earliest day is not for whatever else shares the join
        if let PlanNode::TaskGroup(id) = node {
            let constraint = self.plan.task_groups()[&id].time_constraint();
            if let Some(date) = constraint.bounds_on(NodeEnd::Start).0 {
                start = self.net.add_vertex(Vertex {
                    after: vec![start],
                    earliest: Some(self.day(date)),
                    ..Vertex::default()
                });
            }
        }
        self.visiting.remove(&node);
        self.starts.insert(node, start);
        start
    }

    fn end_of(&mut self, node: PlanNode) -> VertexId {
        match self.ends.get(&node) {
            Some(end) => *end,
            None => self.start_of(node),
        }
    }

    fn wire(&mut self, node: PlanNode) {
        let plan = self.plan;
        let start = self.start_of(node);
        let end = self.end_of(node);
        let mut lowered = Lowered {
            start,
            end,
            arc: None,
        };
        let work = match node {
            PlanNode::Task(id) => self.task(&plan.tasks()[&id]).map(ArcKind::Work),
            PlanNode::Milestone(id) => {
                self.milestone(&plan.milestones()[&id], start, end);
                lowered.start = end;
                None
            }
            PlanNode::PlanBuffer(id) => {
                Some(ArcKind::Buffer(self.buffer(&plan.plan_buffers()[&id])))
            }
            PlanNode::PlanDelay(id) => {
                let days = *plan.plan_delays()[&id].duration();
                Some(ArcKind::Work(Work::wait(days, self.table.full())))
            }
            PlanNode::TaskGroup(id) => self.group(id, start, end),
        };
        if let Some(kind) = work {
            lowered.arc = Some(self.net.add_arc(Arc {
                node,
                plan: self.index,
                start,
                end,
                kind,
            }));
        }
        self.net.lowered.insert((self.index, node), lowered);
    }

    /// Never before today unless it has already happened. The day it is shown on is worked out in
    /// `write`.
    fn milestone(&mut self, milestone: &Milestone, start: VertexId, end: VertexId) {
        let happened = self.statuses[milestone.id()] == DerivedStatus::Completed;
        let now = (!happened || start == self.net.root).then_some(self.now());
        let (earliest, latest) = milestone.time_constraint().bounds();
        let on = |date: Option<NaiveDate>| date.map(|d| self.day(d).next());
        let (earliest, deadline) = (on(earliest), on(latest));
        let vertex = self.net.vertex_mut(end);
        vertex.after = vec![start];
        vertex.earliest = now.max(earliest);
        vertex.deadline = deadline;
    }

    /// With no members it lasts `EMPTY_DAYS`; with only dropped ones it has nothing to end on, so
    /// it is unscheduled. An earliest start is handled in `start_of`, an earliest end in
    /// `end_not_before`.
    fn group(&mut self, id: Uuid, start: VertexId, end: VertexId) -> Option<ArcKind> {
        let constraint = self.plan.task_groups()[&id].time_constraint();
        let by = constraint.bounds_on(NodeEnd::Start).1.map(|d| self.day(d));
        let vertex = self.net.vertex_mut(start);
        vertex.deadline = [vertex.deadline, by].into_iter().flatten().min();
        let by = constraint.bounds_on(NodeEnd::End).1;
        self.net.vertex_mut(end).deadline = by.map(|d| self.day(d).next());
        let members = self
            .items
            .iter()
            .filter(|(_, item)| item.parent == Parent::Node(id));
        let (dropped, visible): (Vec<_>, Vec<_>) = members.partition(|(_, item)| item.dropped);
        if visible.is_empty() && dropped.is_empty() {
            return Some(ArcKind::Work(Work::wait(
                EMPTY_DAYS as u32,
                self.table.full(),
            )));
        }
        let mut after: Vec<VertexId> = visible.iter().map(|(node, _)| self.ends[node]).collect();
        after.sort();
        self.net.vertex_mut(end).after = after;
        None
    }

    /// Applies to the members nothing else in the group depends on, recursing into subgroups
    fn end_not_before(&mut self, group: Uuid, day: Day) {
        let inside = |item: &Item| item.parent == Parent::Node(group) && !item.dropped;
        let members: Vec<PlanNode> = self
            .items
            .iter()
            .filter(|(_, item)| inside(item))
            .map(|(node, _)| *node)
            .collect();
        let followed = |node: &PlanNode| {
            let mut others = self.items.values().filter(|item| inside(item));
            others.any(|item| item.dependencies.contains(&*node))
        };
        let last: Vec<PlanNode> = members.into_iter().filter(|n| !followed(n)).collect();
        for node in last {
            let Some(lowered) = self.net.lowered.get(&(self.index, node)).copied() else {
                continue;
            };
            match (node, lowered.arc) {
                (PlanNode::TaskGroup(inner), None) => self.end_not_before(inner, day),
                (PlanNode::Milestone(_), _) => {
                    let vertex = self.net.vertex_mut(lowered.end);
                    vertex.earliest = vertex.earliest.max(Some(day));
                }
                (_, Some(arc)) => {
                    if let ArcKind::Work(work) = &mut self.net.arcs[arc.0 as usize].kind {
                        work.floor_end = work.floor_end.max(Some(day));
                    }
                }
                (_, None) => {}
            }
        }
    }

    fn buffer(&mut self, buffer: &PlanBuffer) -> Buffer {
        let calendar = self.plan_row();
        let until = match buffer.set_start_day() {
            None => Until::Days(*buffer.duration()),
            Some(set) => {
                let working = self.table.valid(&[], None, calendar);
                let last = working.nth_from(self.day(*set), *buffer.duration());
                let end = match *buffer.duration() {
                    0 => Some(self.day(*set)),
                    _ => last.map(Day::next),
                };
                Until::Day(end.unwrap_or(Day(self.table.days as u32)))
            }
        };
        Buffer {
            until,
            calendar,
            shrink_to: self.shrink_to(PlanNode::PlanBuffer(*buffer.id())),
        }
    }

    /// Earliest hard date of the tasks and milestones that depend on the buffer directly
    fn shrink_to(&self, buffer: PlanNode) -> Option<Day> {
        let depends = |dependencies: &[PlanNode]| dependencies.contains(&buffer);
        let tasks = self.plan.tasks().values();
        let tasks = tasks
            .filter(|t| depends(t.dependencies()))
            .filter_map(|t| t.time_constraint().bounds_on(NodeEnd::Start).1)
            .map(|date| self.day(date));
        let milestones = self.plan.milestones().values();
        let milestones = milestones
            .filter(|m| depends(m.dependencies()))
            .filter_map(|m| m.time_constraint().bounds().1)
            .map(|date| self.day(date).next());
        tasks.chain(milestones).min()
    }

    fn task(&mut self, task: &Task) -> Option<Work> {
        let node = (self.index, PlanNode::Task(*task.id()));
        let state = task.persistent_state();
        let today = self.tenant.today();
        let begun = state.actual_start_date.unwrap_or(today);
        let ended = state.actual_end_date.unwrap_or(today).max(begun);
        let mut work = Work::wait(task.duration(), self.plan_row());
        work.progress = match state.status {
            Status::Dropped => {
                let reason = UnscheduledReason::Dropped;
                self.net.unscheduled.insert(node, reason);
                return None;
            }
            Status::NotStarted => Progress::Open,
            Status::InProgress => Progress::Active {
                begun: self.day(begun),
            },
            Status::Completed | Status::OnHold => Progress::Done {
                begun: self.day(begun),
                ended: self.day(ended).next(),
            },
        };
        if work.is_open() {
            let constraint = task.time_constraint();
            let (earliest, latest) = constraint.bounds_on(NodeEnd::Start);
            work.floor_start = Some(self.now()).max(earliest.map(|d| self.day(d)));
            work.latest_start = latest.map(|d| self.day(d));
            // a date for its last day is the vertex after that day
            let (earliest, latest) = constraint.bounds_on(NodeEnd::End);
            work.floor_end = earliest.map(|d| self.day(d).next());
            work.latest_end = latest.map(|d| self.day(d).next());
        }
        let done = matches!(work.progress, Progress::Done { .. });
        work.slots = match self.slots(task) {
            Ok(slots) => slots,
            // work that has happened keeps its days even if nobody can be put on it now
            Err(_) if done => Vec::new(),
            Err(reason) => {
                self.net.unscheduled.insert(node, reason);
                return None;
            }
        };
        let affinities: BTreeSet<Uuid> = task.affinities().iter().copied().collect();
        work.mask = self.table.mask(self.plan, affinities);
        Some(work)
    }

    fn slots(&self, task: &Task) -> Result<Vec<Slot>, UnscheduledReason> {
        let mut asked: Vec<(Worker, u8)> = task
            .allocations()
            .iter()
            .map(|(worker, hours)| (*worker, hours.get()))
            .collect();
        asked.sort_by_key(|(w, _)| worker_key(w));
        let mut slots = Vec::new();
        let mut fixed_hours: HashMap<RowId, u8> = HashMap::new();
        for (worker, hours) in asked {
            let recorded = task.actual_workers().get(&worker);
            let candidates: Vec<RowId> = match (recorded, worker) {
                (Some(who), _) => self.row(*who).into_iter().collect(),
                (None, Worker::User(user)) => self.row(Assignee::User(user)).into_iter().collect(),
                (None, Worker::Placeholder) => vec![self.plan_row()],
                (None, Worker::Tag(_) | Worker::UserGroup(_)) => {
                    let people = self.eligible(&worker).into_iter();
                    let mut rows: Vec<RowId> =
                        people.filter_map(|u| self.row(Assignee::User(u))).collect();
                    rows.sort();
                    rows
                }
            };
            match candidates.as_slice() {
                [] => return Err(UnscheduledReason::NoEligibleWorkers),
                [one] => {
                    let total = fixed_hours.entry(*one).or_insert(0);
                    *total += hours;
                    if *total > 8 {
                        return Err(UnscheduledReason::NoCapacity);
                    }
                }
                _ => {}
            }
            slots.push(Slot {
                worker,
                hours,
                candidates,
            });
        }
        Ok(slots)
    }

    fn row(&self, who: Assignee) -> Option<RowId> {
        match who {
            Assignee::User(user) => self.table.row(&RowKey::User(user)),
            Assignee::Placeholder => Some(self.plan_row()),
        }
    }

    fn plan_row(&self) -> RowId {
        let key = RowKey::Plan(*self.plan.id());
        self.table.row(&key).expect("every plan has a row")
    }

    fn eligible(&self, worker: &Worker) -> Vec<Uuid> {
        match worker {
            Worker::Tag(tag) => {
                let users = self.plan.users().iter();
                let holders = users.filter(|(_, u)| u.tags().contains(tag));
                holders.map(|(id, _)| *id).collect()
            }
            Worker::UserGroup(group) => self.tenant.group_members(*group).into_iter().collect(),
            Worker::User(_) | Worker::Placeholder => Vec::new(),
        }
    }
}
// }}}

// helpers {{{
fn items<T: PlanItem>(plan: &Plan) -> Vec<(PlanNode, Item)> {
    let mut items: Vec<(PlanNode, Item)> = T::entities(plan)
        .iter()
        .map(|(id, item)| {
            let item = Item {
                parent: AbstractPlanNode::parent(item),
                dependencies: AbstractPlanNode::dependencies(item).to_vec(),
                dropped: false,
            };
            (T::node(*id), item)
        })
        .collect();
    items.sort_by_key(|(node, _)| node.id());
    items
}

/// So choices do not depend on map order
fn worker_key(worker: &Worker) -> (u8, Uuid) {
    match worker {
        Worker::User(id) => (0, *id),
        Worker::Tag(id) => (1, *id),
        Worker::UserGroup(id) => (2, *id),
        Worker::Placeholder => (3, Uuid::nil()),
    }
}
// }}}
