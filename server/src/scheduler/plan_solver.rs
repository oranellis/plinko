// modules {{{
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Days, NaiveDate};
use uuid::Uuid;

use crate::{
    availability::{calendar::Calendar, rule_set::RuleSet},
    multitenancy::tenant::Tenant,
    planner::{
        graph::{self, Vertex},
        plan::Plan,
    },
    scheduler::{
        fill::{Filled, Hours, Slot, Spec, fill},
        ledger::{Claim, Ledger, MAX_FILL_DAYS, Run},
        network::Network,
        state::{ConstraintViolation, PlanVolatileState, TargetForecast, UnscheduledReason},
        status::milestone_statuses,
    },
    types::{
        common::{DateRange, TimeConstraint},
        dependency::{Dependency, PlanNode},
        milestone::{self, DerivedStatus},
        plan_buffer,
        task::{self, Assignee, Status, Task, Worker},
        task_group,
        workload::Workload,
    },
};
// }}}

type Assignments = HashMap<Assignee, BTreeMap<NaiveDate, Workload>>;
type Reason = UnscheduledReason;

/// The most compaction passes, each can only move things earlier
const MAX_COMPACT_PASSES: usize = 50;

#[derive(Clone, Debug)]
struct Placement {
    range: DateRange,
    /// What a task works on, other nodes have none
    work: Option<Work>,
}

#[derive(Clone, Debug)]
struct Work {
    /// What the task holds in the ledger
    run: Run,
    assignments: Assignments,
    workers: HashMap<Worker, Assignee>,
}

/// How a task is placed
#[derive(Clone, Copy)]
enum Kind {
    /// Not started, from the given earliest day
    Open(NaiveDate),
    /// In progress from its actual start, working at least up to today
    Active,
    /// Completed or on hold, fixed to its actual dates
    Past,
}

/// Hours of people, from their calendars and the ledger
struct Availability<'a> {
    tenant: &'a Tenant,
    plan_calendar: &'a Calendar<Workload>,
    ledger: &'a Ledger,
}

impl Hours for Availability<'_> {
    fn cap(&self, who: Assignee, day: NaiveDate) -> Workload {
        let calendar = match who {
            Assignee::Placeholder => Some(self.plan_calendar),
            Assignee::User(user) => self.tenant.calendar(&user),
        };
        calendar.and_then(|c| c.on(day)).unwrap_or_default()
    }

    fn free(&self, who: Assignee, day: NaiveDate) -> Workload {
        match who {
            Assignee::User(user) if day >= self.ledger.start() => {
                Workload::new(self.ledger.free(&user, day)).expect("a ledger holds valid hours")
            }
            _ => self.cap(who, day),
        }
    }
}

/// Schedules one plan against a ledger shared with other plans, by Plinko 1's stages: work that
/// has happened or started, then chains to hard dates, then chains to targets, then the rest.
/// Nodes go in only once everything they depend on is settled, most urgent (longest chain of work
/// to follow) first, and a final pass pulls nodes back into gaps left behind.
pub struct PlanSolver<'a> {
    tenant: &'a Tenant,
    plan: &'a Plan,
    net: Network<'a>,
    order: Vec<Vertex>,
    calendar: Calendar<Workload>,
    today: NaiveDate,
    statuses: HashMap<Uuid, DerivedStatus>,
    /// The latest a buffer may end for what hard dated nodes depend on it for
    limits: HashMap<Uuid, NaiveDate>,
    placed: HashMap<Vertex, Placement>,
    /// Placed, or out of the schedule for good
    settled: HashSet<Vertex>,
    unscheduled: HashMap<PlanNode, Reason>,
}

// impl PlanSolver {{{
impl<'a> PlanSolver<'a> {
    pub fn new(tenant: &'a Tenant, plan: &'a Plan) -> Self {
        let today = tenant.today();
        let calendar = tenant
            .plan_calendar(plan.id())
            .cloned()
            .unwrap_or_else(|| RuleSet::default().calendar(today, today - Days::new(1)));
        let net = Network::new(plan);
        let order = net.order().to_vec();
        let anchored = graph::anchored(plan);
        let mut solver = Self {
            tenant,
            plan,
            net,
            order,
            calendar,
            today,
            statuses: milestone_statuses(plan),
            limits: buffer_limits(plan),
            placed: HashMap::new(),
            settled: HashSet::new(),
            unscheduled: HashMap::new(),
        };
        for v in solver.order.clone() {
            if !anchored.contains(&v) {
                solver.leave_out(v, Reason::NoPathToPlanStart);
            }
        }
        for (id, task) in plan.tasks() {
            if task.persistent_state().status == Status::Dropped {
                solver
                    .unscheduled
                    .insert(PlanNode::Task(*id), Reason::Dropped);
            }
        }
        solver
    }

    /// Schedules the plan, taking its hours from the ledger
    pub fn solve(mut self, ledger: &mut Ledger) -> PlanVolatileState {
        self.place_status(ledger, |s| matches!(s, Status::Completed | Status::OnHold));
        self.place_status(ledger, |s| s == Status::InProgress);
        for node in self.hard_dated() {
            let chain = self.net.ancestors([node]);
            self.stage(&chain, ledger);
        }
        let targets = self.plan.targets().values();
        let targets: Vec<Vertex> = targets.map(|t| Vertex::Node(t.node())).collect();
        if !targets.is_empty() {
            let chain = self.net.ancestors(targets);
            self.stage(&chain, ledger);
        }
        let everything = self.order.iter().copied().collect();
        self.stage(&everything, ledger);
        self.compact(ledger);
        self.finish()
    }

    fn status(&self, v: &Vertex) -> Option<Status> {
        match v {
            Vertex::Node(PlanNode::Task(id)) => {
                Some(self.plan.tasks()[id].persistent_state().status)
            }
            _ => None,
        }
    }

    fn is_open(&self, v: &Vertex) -> bool {
        self.status(v) == Some(Status::NotStarted)
    }

    /// Nodes whose place follows from their dependencies alone, taking no hours
    fn is_derived(&self, v: &Vertex) -> bool {
        self.status(v).is_none_or(|s| s == Status::Dropped)
    }

    /// Hidden from the result: dropped tasks only mark the place their dependents start from
    fn visible(&self, node: &PlanNode) -> bool {
        self.status(&Vertex::Node(*node)) != Some(Status::Dropped)
    }

    fn leave_out(&mut self, v: Vertex, reason: Reason) {
        self.settled.insert(v);
        if let Vertex::Node(node) = v {
            self.unscheduled.entry(node).or_insert(reason);
        }
    }

    fn ready(&self, v: &Vertex) -> bool {
        let settled = |e: &graph::Edge| e.from.is_none_or(|u| self.settled.contains(&u));
        self.net.deps(v).iter().all(settled)
    }

    // dates {{{
    fn end_of(&self, node: PlanNode) -> Option<NaiveDate> {
        Some(self.placed.get(&Vertex::Node(node))?.range.end())
    }

    /// The day the dependencies of `v` that were placed allow it, `None` if there are none. A
    /// milestone completing in the past may start from the plan start date, others from today.
    fn dependency_date(&self, v: &Vertex) -> Option<NaiveDate> {
        let milestone = matches!(v, Vertex::Node(PlanNode::Milestone(_)));
        let start = *self.plan.scheduler_data().start_date();
        let plan_start = match v {
            Vertex::Node(PlanNode::Milestone(id))
                if self.statuses[id] == DerivedStatus::Completed =>
            {
                start
            }
            _ => start.max(self.today),
        };
        let dates = self.net.deps(v).iter().filter_map(|e| {
            let dep = Dependency {
                lag: e.lag,
                ..Default::default()
            };
            match e.from {
                None => Some(dep.start_after(plan_start)),
                Some(g @ Vertex::GroupStart(_)) => {
                    Some(dep.start_after(self.placed.get(&g)?.range.start()))
                }
                Some(Vertex::Node(n)) if milestone => {
                    self.end_of(n).map(|e| dep.milestone_date_after(e))
                }
                Some(Vertex::Node(n)) => self.end_of(n).map(|e| dep.start_after(e)),
            }
        });
        dates.max().map(|d| d.max(start))
    }

    /// Where a not started task may start: after its dependencies, and not before tomorrow
    fn open_start(&self, v: &Vertex, task: &Task) -> Option<NaiveDate> {
        let earliest = self.dependency_date(v)?.max(self.today + Days::new(1));
        Some(task.time_constraint().apply(earliest).start)
    }

    /// The place of a node that follows from where its dependencies are
    fn derive(&self, v: &Vertex) -> Result<Placement, Reason> {
        let date = || self.dependency_date(v).ok_or(Reason::DependsOnUnscheduled);
        let day =
            |d: NaiveDate, days: u64| DateRange::new(d, d + Days::new(days)).expect("forwards");
        let range = match v {
            Vertex::GroupStart(_) | Vertex::Node(PlanNode::Task(_)) => day(date()?, 0),
            Vertex::Node(PlanNode::Milestone(id)) => {
                let mut landing = date()?;
                if self.statuses[id] != DerivedStatus::Completed {
                    landing = landing.max(self.today);
                }
                let constraint = self.plan.milestones()[id].time_constraint();
                day(constraint.apply(landing).start, 1)
            }
            Vertex::Node(PlanNode::PlanBuffer(id)) => {
                let buffer = &self.plan.plan_buffers()[id];
                buffer
                    .place(date()?, &self.calendar, self.limits.get(id).copied())
                    .ok_or(Reason::NoCapacity)?
            }
            Vertex::Node(PlanNode::TaskGroup(id)) => {
                let start = self.placed.get(&Vertex::GroupStart(*id));
                let start = start.ok_or(Reason::DependsOnUnscheduled)?.range.start();
                let all = self.net.members(id);
                let scheduled: Vec<DateRange> = all
                    .iter()
                    .filter(|n| self.visible(n))
                    .filter_map(|n| Some(self.placed.get(&Vertex::Node(*n))?.range))
                    .collect();
                if scheduled.is_empty() && !all.is_empty() {
                    return Err(Reason::NoScheduledMembers);
                }
                let group = &self.plan.task_groups()[id];
                group
                    .place(start, &scheduled, &self.calendar)
                    .ok_or(Reason::NoCapacity)?
            }
        };
        Ok(Placement { range, work: None })
    }

    /// Places every node that follows from its dependencies and has them all settled
    fn settle(&mut self) {
        for i in 0..self.order.len() {
            let v = self.order[i];
            if self.settled.contains(&v) || !self.is_derived(&v) || !self.ready(&v) {
                continue;
            }
            match self.derive(&v) {
                Ok(placement) => {
                    self.placed.insert(v, placement);
                    self.settled.insert(v);
                }
                Err(reason) => self.leave_out(v, reason),
            }
        }
    }

    /// Moves the derived nodes to follow the tasks after some moved
    fn refresh_derived(&mut self) {
        for i in 0..self.order.len() {
            let v = self.order[i];
            if self.is_derived(&v) && self.placed.contains_key(&v) {
                let derived = self.derive(&v);
                self.placed.extend(derived.map(|p| (v, p)));
            }
        }
    }
    // }}}

    // tasks {{{
    fn candidates(&self, worker: &Worker) -> Vec<Uuid> {
        let mut people: Vec<Uuid> = match worker {
            Worker::Tag(tag) => {
                let users = self.plan.users().iter();
                users
                    .filter(|(_, u)| u.tags().contains(tag))
                    .map(|(id, _)| *id)
                    .collect()
            }
            Worker::UserGroup(group) => self.tenant.group_members(*group).into_iter().collect(),
            Worker::User(_) | Worker::Placeholder => Vec::new(),
        };
        people.sort();
        people
    }

    /// The people for each worker the task asks for. A recorded person is used as is, as is the
    /// named user. For a tag or group it is whoever finishes first, in a fixed order of workers,
    /// never someone already on the task.
    fn resolve(
        &self,
        task: &Task,
        spec: &Spec,
        avail: &Availability,
        partial_ok: bool,
    ) -> Result<(Vec<Slot>, HashMap<Worker, Assignee>), Reason> {
        let mut asked: Vec<(Worker, Workload)> =
            task.allocations().iter().map(|(w, h)| (*w, *h)).collect();
        asked.sort_by_key(|(w, _)| worker_key(w));
        let (mut workers, mut slots, mut pending) = (HashMap::new(), Vec::new(), Vec::new());
        for (worker, hours) in asked {
            let named = match worker {
                Worker::User(user) => Some(Assignee::User(user)),
                Worker::Placeholder => Some(Assignee::Placeholder),
                Worker::Tag(_) | Worker::UserGroup(_) => None,
            };
            match task.actual_workers().get(&worker).copied().or(named) {
                Some(who) => {
                    workers.insert(worker, who);
                    slots.push(Slot { who, hours });
                }
                None => pending.push((worker, hours)),
            }
        }
        let mut used: HashSet<Uuid> = slots
            .iter()
            .filter_map(|s| match s.who {
                Assignee::User(u) => Some(u),
                Assignee::Placeholder => None,
            })
            .collect();
        for (worker, hours) in pending {
            let candidates: Vec<Uuid> = self
                .candidates(&worker)
                .into_iter()
                .filter(|u| !used.contains(u))
                .collect();
            if candidates.is_empty() {
                return Err(Reason::NoEligibleWorkers);
            }
            let finish = |user: Uuid| {
                let mut trial: Vec<Slot> = slots
                    .iter()
                    .filter(|s| s.hours.get() > 0)
                    .copied()
                    .collect();
                trial.push(Slot {
                    who: Assignee::User(user),
                    hours,
                });
                let filled = fill(avail, &trial, spec);
                (filled.complete || partial_ok)
                    .then(|| filled.last().map_or(spec.start, |l| l + Days::new(1)))
            };
            let best = candidates
                .into_iter()
                .filter_map(|user| Some((finish(user)?, user)))
                .min()
                .ok_or(Reason::NoCapacity)?;
            used.insert(best.1);
            workers.insert(worker, Assignee::User(best.1));
            slots.push(Slot {
                who: Assignee::User(best.1),
                hours,
            });
        }
        Ok((merge(slots)?, workers))
    }

    /// Where a task goes, without taking anything from the ledger
    fn plan_task(&self, task: &Task, kind: Kind, ledger: &Ledger) -> Result<Placement, Reason> {
        let state = task.persistent_state();
        let begun = state.actual_start_date.unwrap_or(self.today);
        let ended = state.actual_end_date.unwrap_or(self.today).max(begun);
        let horizon =
            |start: NaiveDate| (start + Days::new(u64::from(MAX_FILL_DAYS))).min(ledger.end());
        let (start, stop, through) = match kind {
            Kind::Open(start) => (start, horizon(start), None),
            Kind::Active => (begun, horizon(begun), Some(self.today)),
            Kind::Past => (begun, ended + Days::new(1), None),
        };
        let allowed = |day| self.plan.task_allows_day(task.id(), day).unwrap_or(true);
        let spec = Spec {
            mode: task.mode(),
            duration: task.duration(),
            start,
            stop,
            keep_before: self.today,
            through,
            allowed: &allowed,
        };
        let avail = Availability {
            tenant: self.tenant,
            plan_calendar: &self.calendar,
            ledger,
        };
        let past = matches!(kind, Kind::Past);
        let (slots, workers) = match self.resolve(task, &spec, &avail, past) {
            Ok(resolved) => resolved,
            Err(_) if past => Default::default(),
            Err(reason) => return Err(reason),
        };
        // a task with no one to work it still takes its duration of the plan's working days
        let stand_in = [Slot {
            who: Assignee::Placeholder,
            hours: Workload::new(1).expect("one hour"),
        }];
        let filled = fill(
            &avail,
            if slots.is_empty() { &stand_in } else { &slots },
            &spec,
        );
        if !filled.complete && !past {
            return Err(Reason::NoCapacity);
        }
        let first = filled.first();
        let worked_to = filled.last().map(|l| l + Days::new(1));
        let latest = |extra: Option<NaiveDate>| [worked_to, extra].into_iter().flatten().max();
        let (from, to) = match kind {
            Kind::Open(_) => {
                let from = first.ok_or(Reason::NoCapacity)?;
                (from, latest(None).unwrap_or(from + Days::new(1)))
            }
            Kind::Active => (
                begun,
                latest(Some(self.today + Days::new(1))).expect("a day"),
            ),
            Kind::Past => (begun, ended + Days::new(1)),
        };
        Ok(Placement {
            range: DateRange::new(from, to).ok_or(Reason::NoCapacity)?,
            work: Some(build_work(&slots, filled, workers)),
        })
    }

    /// Takes the placement's hours from the ledger
    fn take(&self, ledger: &mut Ledger, placement: Placement) -> Result<Placement, Reason> {
        let run = placement.work.as_ref().map(|w| &w.run);
        match run.is_none_or(|r| ledger.claim(r)) {
            true => Ok(placement),
            false => Err(Reason::NoCapacity),
        }
    }

    /// Places the tasks with a status that fixes their start, most urgent first
    fn place_status(&mut self, ledger: &mut Ledger, wanted: impl Fn(Status) -> bool) {
        let mut tasks: Vec<Vertex> = self
            .order
            .iter()
            .filter(|v| !self.settled.contains(v))
            .filter(|v| self.status(v).is_some_and(&wanted))
            .copied()
            .collect();
        tasks.sort_by_key(|v| (Reverse(self.net.remaining(v)), v.key()));
        for v in tasks {
            let Vertex::Node(PlanNode::Task(id)) = v else {
                continue;
            };
            let task = &self.plan.tasks()[&id];
            let kind = match task.persistent_state().status {
                Status::InProgress => Kind::Active,
                _ => Kind::Past,
            };
            let placed = self
                .plan_task(task, kind, ledger)
                .and_then(|p| self.take(ledger, p));
            self.put(v, placed);
        }
    }

    fn put(&mut self, v: Vertex, placement: Result<Placement, Reason>) {
        match placement {
            Ok(placement) => {
                self.placed.insert(v, placement);
                self.settled.insert(v);
            }
            Err(reason) => self.leave_out(v, reason),
        }
    }

    /// Tasks and milestones with a date they must not pass, soonest first
    fn hard_dated(&self) -> Vec<Vertex> {
        let tasks = self
            .plan
            .tasks()
            .iter()
            .map(|(id, t)| (Vertex::Node(PlanNode::Task(*id)), t.time_constraint()));
        let milestones = self
            .plan
            .milestones()
            .iter()
            .map(|(id, m)| (Vertex::Node(PlanNode::Milestone(*id)), m.time_constraint()));
        let mut dated: Vec<(NaiveDate, (u8, Uuid), Vertex)> = tasks
            .chain(milestones)
            .filter_map(|(v, c)| match c {
                TimeConstraint::Fixed(d) | TimeConstraint::Latest(d) => Some((d, v.key(), v)),
                _ => None,
            })
            .collect();
        dated.sort_by_key(|(date, key, _)| (*date, *key));
        dated.into_iter().map(|(_, _, v)| v).collect()
    }

    /// Places not started tasks from `chain`, once everything they depend on is settled, the
    /// longest chain of work to follow first
    fn stage(&mut self, chain: &HashSet<Vertex>, ledger: &mut Ledger) {
        loop {
            self.settle();
            let next = chain
                .iter()
                .filter(|v| self.is_open(v) && !self.settled.contains(v) && self.ready(v))
                .max_by_key(|v| (self.net.remaining(v), Reverse(v.key())))
                .copied();
            let Some(v) = next else {
                break;
            };
            let Vertex::Node(PlanNode::Task(id)) = v else {
                continue;
            };
            let task = &self.plan.tasks()[&id];
            let placed = self
                .open_start(&v, task)
                .ok_or(Reason::DependsOnUnscheduled)
                .and_then(|start| self.plan_task(task, Kind::Open(start), ledger))
                .and_then(|p| self.take(ledger, p));
            self.put(v, placed);
        }
    }

    /// Pulls tasks into room left by others until nothing ends sooner
    fn compact(&mut self, ledger: &mut Ledger) {
        for _ in 0..MAX_COMPACT_PASSES {
            if !self.compact_pass(ledger) {
                break;
            }
        }
    }

    fn compact_pass(&mut self, ledger: &mut Ledger) -> bool {
        let mut tasks: Vec<(NaiveDate, Vertex)> = self
            .order
            .iter()
            .filter(|v| self.is_open(v))
            .filter_map(|v| Some((self.placed.get(v)?.range.start(), *v)))
            .collect();
        tasks.sort_by_key(|(start, v)| (*start, v.key()));
        let mut moved = false;
        for (_, v) in tasks {
            let Vertex::Node(PlanNode::Task(id)) = v else {
                continue;
            };
            let task = &self.plan.tasks()[&id];
            let old = self.placed[&v].clone();
            let Some(start) = self.open_start(&v, task).filter(|s| *s < old.range.end()) else {
                continue;
            };
            let old_run = old.work.map(|w| w.run).unwrap_or_default();
            ledger.release(&old_run);
            let better = self
                .plan_task(task, Kind::Open(start), ledger)
                .ok()
                .filter(|p| p.range.end() < self.placed[&v].range.end())
                .and_then(|p| self.take(ledger, p).ok());
            match better {
                Some(placement) => {
                    self.placed.insert(v, placement);
                    self.refresh_derived();
                    moved = true;
                }
                None => {
                    ledger.claim(&old_run);
                }
            }
        }
        moved
    }
    // }}}

    // result {{{
    fn finish(self) -> PlanVolatileState {
        let mut out = PlanVolatileState::default();
        for (v, placement) in &self.placed {
            let Vertex::Node(node) = v else {
                continue;
            };
            if !self.visible(node) {
                continue;
            }
            let date_range = placement.range;
            match node {
                PlanNode::Task(id) => {
                    let work = placement.work.clone().expect("tasks have work");
                    let state = task::VolatileState {
                        date_range,
                        assignments: work.assignments,
                        workers: work.workers,
                    };
                    out.tasks.insert(*id, state);
                    let constraint = self.plan.tasks()[id].time_constraint();
                    if self.is_open(v) {
                        out.violation(*node, constraint, date_range.start());
                    }
                }
                PlanNode::Milestone(id) => {
                    let date = date_range.start();
                    let derived_status = self.statuses[id];
                    out.milestones.insert(
                        *id,
                        milestone::VolatileState {
                            derived_status,
                            date,
                        },
                    );
                    out.violation(*node, self.plan.milestones()[id].time_constraint(), date);
                }
                PlanNode::PlanBuffer(id) => {
                    out.plan_buffers
                        .insert(*id, plan_buffer::VolatileState { date_range });
                }
                PlanNode::TaskGroup(id) => {
                    out.task_groups
                        .insert(*id, task_group::VolatileState { date_range });
                }
            }
        }
        for (id, target) in self.plan.targets() {
            let Some(range) = self
                .placed
                .get(&Vertex::Node(target.node()))
                .map(|p| p.range)
            else {
                continue;
            };
            let last = range
                .end()
                .pred_opt()
                .filter(|d| *d >= range.start())
                .unwrap_or(range.start());
            // on its own the plan is its own baseline
            let forecast = TargetForecast {
                forecast: last,
                baseline: last,
            };
            out.targets.insert(*id, forecast);
        }
        out.unscheduled = self.unscheduled;
        out
    }
    // }}}
}
// }}}

impl PlanVolatileState {
    /// Records a violation if a hard date is passed by the start of the node
    fn violation(&mut self, node: PlanNode, constraint: TimeConstraint, start: NaiveDate) {
        let (TimeConstraint::Fixed(date) | TimeConstraint::Latest(date)) = constraint else {
            return;
        };
        if start > date {
            let scheduled = start;
            let violation = ConstraintViolation {
                constraint,
                scheduled,
            };
            self.constraint_violations.insert(node, violation);
        }
    }
}

/// A fixed order of workers, so choices do not depend on map order
fn worker_key(worker: &Worker) -> (u8, Uuid) {
    match worker {
        Worker::User(id) => (0, *id),
        Worker::Tag(id) => (1, *id),
        Worker::UserGroup(id) => (2, *id),
        Worker::Placeholder => (3, Uuid::nil()),
    }
}

/// One slot for each person, adding up the hours of several requests, and none for no hours
fn merge(slots: Vec<Slot>) -> Result<Vec<Slot>, Reason> {
    let mut merged: Vec<Slot> = Vec::new();
    for slot in slots.into_iter().filter(|s| s.hours.get() > 0) {
        match merged.iter_mut().find(|m| m.who == slot.who) {
            Some(m) => {
                m.hours = Workload::new(m.hours.get() + slot.hours.get())
                    .map_err(|_| Reason::NoCapacity)?
            }
            None => merged.push(slot),
        }
    }
    Ok(merged)
}

/// The hours a fill put on the slots, as assignments and the claims on the ledger. A task with
/// no slots has none.
fn build_work(slots: &[Slot], filled: Filled, workers: HashMap<Worker, Assignee>) -> Work {
    let mut assignments = Assignments::new();
    let mut run = Run::default();
    for (slot, days) in slots.iter().zip(filled.days) {
        for (day, hours) in &days {
            if let Assignee::User(user) = slot.who {
                run.0.push(Claim {
                    user,
                    day: *day,
                    hours: hours.get(),
                });
            }
        }
        assignments.insert(slot.who, days);
    }
    Work {
        run,
        assignments,
        workers,
    }
}

/// The latest a buffer may end for the tasks and milestones that depend on it and cannot move
/// later, the smallest where several do
fn buffer_limits(plan: &Plan) -> HashMap<Uuid, NaiveDate> {
    let mut limits: HashMap<Uuid, NaiveDate> = HashMap::new();
    let tasks = plan
        .tasks()
        .values()
        .map(|t| (t.time_constraint(), t.dependencies(), 0));
    let milestones = plan
        .milestones()
        .values()
        .map(|m| (m.time_constraint(), m.dependencies(), 1));
    for (constraint, dependencies, extra) in tasks.chain(milestones) {
        let (TimeConstraint::Fixed(date) | TimeConstraint::Latest(date)) = constraint else {
            continue;
        };
        for dep in dependencies {
            if let crate::types::dependency::DependencyNode::Node(PlanNode::PlanBuffer(id)) =
                dep.node
            {
                let limit = date - chrono::Duration::days(i64::from(dep.lag))
                    + chrono::Duration::days(extra);
                let slot = limits.entry(id).or_insert(limit);
                *slot = (*slot).min(limit);
            }
        }
    }
    limits
}
