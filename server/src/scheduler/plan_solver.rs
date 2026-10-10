// modules {{{
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

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
        slack::late_ends,
        state::{
            Baseline, ConstraintViolation, PlanVolatileState, TargetForecast, TargetSlack,
            UnscheduledReason,
        },
        status::milestone_statuses,
    },
    types::{
        common::{DateRange, TimeConstraint},
        dependency::{Dependency, PlanNode},
        milestone::{self, DerivedStatus},
        plan_buffer,
        task::{self, Assignee, ScheduleMode, Status, Task, Worker},
        task_group,
        workload::Workload,
    },
};
// }}}

type Assignments = HashMap<Assignee, BTreeMap<NaiveDate, Workload>>;
type Reason = UnscheduledReason;

/// The most compaction passes, each can only move things earlier
const MAX_COMPACT_PASSES: usize = 50;

/// Days later a person may finish a task for being much less booked than the first to finish
const SPREAD_DAYS: u64 = 1;

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

impl Availability<'_> {
    /// Hours the person has booked from `from` up to `to`
    fn booked(&self, user: Uuid, from: NaiveDate, to: NaiveDate) -> u32 {
        let days = from.iter_days().take_while(|d| *d < to);
        let booked = |d| self.ledger.capacity(&user, d) - self.ledger.free(&user, d);
        days.map(|d| u32::from(booked(d))).sum()
    }
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
    /// The plan on its own, whose choices of people are kept where nothing better is free
    baseline: Option<&'a Baseline>,
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
    /// Days added to the urgency of a target, `None` for the plan end, by repair rounds
    bumps: HashMap<Option<Uuid>, i64>,
    /// People forced on a task's tag or group slot by repair rounds
    forced: HashMap<(Uuid, Worker), Uuid>,
    /// Days repair rounds take off a hard date, for nodes that missed it, so the chain to it
    /// goes earlier
    tighten: HashMap<PlanNode, i64>,
    /// With selective ordering, the hard dated nodes whose chains go first
    urgent: HashSet<PlanNode>,
    /// What is waiting for what, so what has become ready is known without looking at everything
    ready: Readiness,
    /// The open tasks and milestones with a hard date, and the date
    dated: HashMap<Vertex, NaiveDate>,
}

/// Which nodes have everything they depend on settled, kept up to date as nodes settle, by their
/// place in the order of the plan
#[derive(Default)]
struct Readiness {
    index: HashMap<Vertex, usize>,
    /// What waits for each node, once for each dependency
    after: Vec<Vec<usize>>,
    /// How many dependencies of each node are not settled yet
    waiting: Vec<usize>,
    /// Nodes that follow from their dependencies alone, ready to be placed, the first in order first
    derived: BinaryHeap<Reverse<usize>>,
    /// Not started tasks that are ready
    open: BTreeSet<usize>,
}

impl Readiness {
    fn new(order: &[Vertex], net: &Network) -> Self {
        let index: HashMap<Vertex, usize> = order.iter().enumerate().map(|(i, v)| (*v, i)).collect();
        let mut after = vec![Vec::new(); order.len()];
        let mut waiting = vec![0; order.len()];
        for (i, v) in order.iter().enumerate() {
            for edge in net.deps(v) {
                if let Some(from) = edge.from {
                    after[index[&from]].push(i);
                    waiting[i] += 1;
                }
            }
        }
        Self {
            index,
            after,
            waiting,
            ..Self::default()
        }
    }
}

// impl PlanSolver {{{
impl<'a> PlanSolver<'a> {
    pub fn new(tenant: &'a Tenant, plan: &'a Plan, baseline: Option<&'a Baseline>) -> Self {
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
            baseline,
            net,
            order,
            calendar,
            today,
            statuses: milestone_statuses(plan),
            limits: buffer_limits(plan),
            placed: HashMap::new(),
            settled: HashSet::new(),
            unscheduled: HashMap::new(),
            bumps: HashMap::new(),
            forced: HashMap::new(),
            tighten: HashMap::new(),
            urgent: HashSet::new(),
            ready: Readiness::default(),
            dated: HashMap::new(),
        };
        solver.dated = solver.find_hard_dates();
        solver.ready = Readiness::new(&solver.order, &solver.net);
        for i in 0..solver.order.len() {
            if solver.ready.waiting[i] == 0 {
                solver.queue(i);
            }
        }
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

    /// Applies what a repair round changes: days of urgency for targets and people for slots
    pub fn repair(
        &mut self,
        bumps: HashMap<Option<Uuid>, i64>,
        forced: HashMap<(Uuid, Worker), Uuid>,
        tighten: HashMap<PlanNode, i64>,
        urgent: HashSet<PlanNode>,
    ) {
        self.bumps = bumps;
        self.forced = forced;
        self.tighten = tighten;
        self.urgent = urgent;
    }

    pub fn is_urgent(&self, v: &Vertex) -> bool {
        matches!(v, Vertex::Node(n) if self.urgent.contains(n))
    }

    /// The days a hard dated node must start before its date to make up for missing it
    pub fn tightening(&self, v: &Vertex) -> i64 {
        match v {
            Vertex::Node(node) => self.tighten.get(node).copied().unwrap_or(0),
            _ => 0,
        }
    }

    /// The days of work that follow a vertex and including it
    pub fn remaining(&self, v: &Vertex) -> i64 {
        self.net.remaining(v)
    }

    /// Whether a node is placed or out of the schedule for good
    pub fn is_settled(&self, v: &Vertex) -> bool {
        self.settled.contains(v)
    }

    /// Schedules the plan with the ledger to itself, and works out how much slack each node has
    pub fn solve_alone(mut self, ledger: &mut Ledger) -> Baseline {
        self.run(ledger);
        let measured = self.measure();
        Baseline {
            state: self.finish(),
            ..measured
        }
    }

    fn run(&mut self, ledger: &mut Ledger) {
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

    /// Marks a node settled and queues what that makes ready
    fn settle_vertex(&mut self, v: Vertex) {
        if !self.settled.insert(v) {
            return;
        }
        let i = self.ready.index[&v];
        self.ready.open.remove(&i);
        for k in 0..self.ready.after[i].len() {
            let next = self.ready.after[i][k];
            self.ready.waiting[next] -= 1;
            if self.ready.waiting[next] == 0 {
                self.queue(next);
            }
        }
    }

    /// Puts a node with everything it depends on settled where it waits to be placed
    fn queue(&mut self, i: usize) {
        let v = self.order[i];
        if self.settled.contains(&v) {
            return;
        }
        if self.is_derived(&v) {
            self.ready.derived.push(Reverse(i));
        } else if self.is_open(&v) {
            self.ready.open.insert(i);
        }
    }

    fn leave_out(&mut self, v: Vertex, reason: Reason) {
        self.settle_vertex(v);
        if let Vertex::Node(node) = v {
            self.unscheduled.entry(node).or_insert(reason);
        }
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
                group.place(start, &scheduled).ok_or(Reason::NoCapacity)?
            }
        };
        Ok(Placement { range, work: None })
    }

    /// Places every node that follows from its dependencies and has them all settled
    fn settle(&mut self) {
        while let Some(Reverse(i)) = self.ready.derived.pop() {
            let v = self.order[i];
            if self.settled.contains(&v) {
                continue;
            }
            match self.derive(&v) {
                Ok(placement) => {
                    self.placed.insert(v, placement);
                    self.settle_vertex(v);
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
            let forced = self.forced.get(&(*task.id(), worker));
            let forced = forced.map(|user| Assignee::User(*user));
            match task
                .actual_workers()
                .get(&worker)
                .copied()
                .or(named)
                .or(forced)
            {
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
            let candidates: Vec<Uuid> = eligible(self.tenant, self.plan, &worker)
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
            let kept = self.kept_choice(task, &worker);
            let finishes: Vec<(NaiveDate, bool, Uuid)> = candidates
                .into_iter()
                .filter_map(|user| Some((finish(user)?, Some(user) != kept, user)))
                .collect();
            let (best_end, _, best_user) = *finishes.iter().min().ok_or(Reason::NoCapacity)?;
            // someone much less booked is worth finishing a little later for
            let window = (spec.start, best_end + Days::new(SPREAD_DAYS));
            let booked = |user: Uuid| avail.booked(user, window.0, window.1);
            let spared = finishes
                .iter()
                .filter(|(end, _, _)| *end <= window.1)
                .map(|(end, _, user)| (booked(*user), *end, *user))
                .filter(|(load, _, _)| load * 2 <= booked(best_user) && *load < booked(best_user))
                .min();
            let best = spared.map_or((best_end, best_user), |(_, end, user)| (end, user));
            used.insert(best.1);
            workers.insert(worker, Assignee::User(best.1));
            slots.push(Slot {
                who: Assignee::User(best.1),
                hours,
            });
        }
        Ok((merge(slots)?, workers))
    }

    /// The person the plan on its own chose for the worker, among equals they stay
    fn kept_choice(&self, task: &Task, worker: &Worker) -> Option<Uuid> {
        let solo = self.baseline?.state.tasks.get(task.id())?;
        match solo.workers.get(worker)? {
            Assignee::User(user) => Some(*user),
            Assignee::Placeholder => None,
        }
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
                self.settle_vertex(v);
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
            let ready = self.ready_open();
            let next = ready
                .iter()
                .filter(|v| chain.contains(v))
                .max_by_key(|v| (self.net.remaining(v), Reverse(v.key())))
                .copied();
            let Some(v) = next else {
                break;
            };
            self.place_open(v, ledger);
        }
    }

    /// Places a not started task that is ready, from where its dependencies allow
    pub fn place_open(&mut self, v: Vertex, ledger: &mut Ledger) {
        let Vertex::Node(PlanNode::Task(id)) = v else {
            return;
        };
        let task = &self.plan.tasks()[&id];
        let placed = self
            .open_start(&v, task)
            .ok_or(Reason::DependsOnUnscheduled)
            .and_then(|start| self.plan_task(task, Kind::Open(start), ledger))
            .and_then(|p| self.take(ledger, p));
        self.put(v, placed);
    }

    /// Pulls tasks into room left by others until nothing ends sooner
    pub fn compact(&mut self, ledger: &mut Ledger) {
        for _ in 0..MAX_COMPACT_PASSES {
            if !self.compact_pass(ledger) {
                break;
            }
        }
    }

    fn compact_pass(&mut self, ledger: &mut Ledger) -> bool {
        let mut moved = false;
        for (_, v) in self.compactable() {
            moved |= self.try_compact(v, ledger);
        }
        moved
    }

    /// The placed not started tasks and when they start
    pub fn compactable(&self) -> Vec<(NaiveDate, Vertex)> {
        let open = self.order.iter().filter(|v| self.is_open(v));
        open.filter_map(|v| Some((self.placed.get(v)?.range.start(), *v)))
            .collect()
    }

    /// Places the task again from the earliest day its dependencies allow and keeps that if it
    /// ends sooner, `true` if it did
    pub fn try_compact(&mut self, v: Vertex, ledger: &mut Ledger) -> bool {
        let Vertex::Node(PlanNode::Task(id)) = v else {
            return false;
        };
        let task = &self.plan.tasks()[&id];
        let old = self.placed[&v].clone();
        // a strict run already at the earliest start can only end sooner by starting later
        let earliest = self.open_start(&v, task);
        let tight = |s: &NaiveDate| match task.mode() {
            ScheduleMode::Strict => old.range.start() > *s,
            ScheduleMode::Relaxed => *s < old.range.end(),
        };
        let Some(start) = earliest.filter(tight) else {
            return false;
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
                true
            }
            None => {
                ledger.claim(&old_run);
                false
            }
        }
    }
    // }}}

    // combined runs {{{
    /// Places the work that has happened or started, the first thing a combined run does for
    /// every plan: finished and held work, then work in progress
    pub fn claim_history(&mut self, ledger: &mut Ledger, in_progress: bool) {
        if in_progress {
            self.place_status(ledger, |s| s == Status::InProgress);
        } else {
            self.place_status(ledger, |s| matches!(s, Status::Completed | Status::OnHold));
        }
    }

    /// The not started tasks everything they depend on has settled for, after placing what
    /// follows from dependencies alone
    pub fn ready_open(&self) -> Vec<Vertex> {
        self.ready.open.iter().map(|i| self.order[*i]).collect()
    }

    /// Settles what follows from the placed and returns the tasks ready to place
    pub fn advance(&mut self) -> Vec<Vertex> {
        self.settle();
        self.ready_open()
    }

    /// Gives up on tasks that cannot become ready, and places what follows from that
    pub fn sweep_stuck(&mut self) {
        let stuck: Vec<Vertex> = self
            .order
            .iter()
            .filter(|v| self.is_open(v) && !self.settled.contains(v))
            .copied()
            .collect();
        for v in stuck {
            self.leave_out(v, Reason::DependsOnUnscheduled);
        }
        self.settle();
    }

    /// The slip of a task whose dependencies are all placed, without working out the rest
    pub fn ready_slip(&self, v: &Vertex) -> i64 {
        let alone = self.baseline.and_then(|b| b.ranges.get(v));
        alone.map_or(0, |a| {
            self.slip_with(v, a, |u| Some(self.placed.get(u)?.range.end()))
        })
    }

    /// Days later than `alone` the node starts given where what it depends on ends
    fn slip_with(
        &self,
        v: &Vertex,
        alone: &DateRange,
        end: impl Fn(&Vertex) -> Option<NaiveDate>,
    ) -> i64 {
        let milestone = matches!(v, Vertex::Node(PlanNode::Milestone(_)));
        let group = matches!(v, Vertex::Node(PlanNode::TaskGroup(_)));
        let dates = self.net.deps(v).iter().filter_map(|e| {
            let from_end = end(&e.from?)?;
            let dep = Dependency {
                lag: e.lag,
                ..Default::default()
            };
            Some(match (group, milestone) {
                (true, _) => from_end,
                (_, true) => dep.milestone_date_after(from_end),
                _ => dep.start_after(from_end),
            })
        });
        let slip = match dates.max() {
            // a group ends when its members do
            Some(latest) if group => (latest - alone.end()).num_days(),
            Some(latest) => (latest - alone.start()).num_days(),
            None => 0,
        };
        slip.max(0)
    }

    /// The day after a ready task would end if it were placed now, without taking anything
    fn end_if_placed(&self, v: &Vertex, ledger: &Ledger) -> Option<NaiveDate> {
        let Vertex::Node(PlanNode::Task(id)) = v else {
            return None;
        };
        let task = &self.plan.tasks()[id];
        let start = self.open_start(v, task)?;
        let placement = self.plan_task(task, Kind::Open(start), ledger).ok()?;
        Some(placement.range.end())
    }

    /// A ready task with a hard date that placing `next` first would put in danger: `next` would
    /// run past the day the dated task must start by, and if they share a person that task could
    /// then start no sooner than `next` ends. Gives the dated task, its date and how many days
    /// `next` would run past it, for the earliest date.
    pub fn pressed(
        &self,
        ready: &[Vertex],
        next: &Vertex,
        ledger: &Ledger,
    ) -> Option<(Vertex, NaiveDate, i64)> {
        let dates = self.hard_dates();
        let mut dated: Vec<(NaiveDate, Vertex)> = ready
            .iter()
            .filter(|v| *v != next)
            .filter_map(|v| {
                let date = *dates.get(v)? - chrono::Duration::days(self.tightening(v));
                Some((date, *v))
            })
            .collect();
        if dated.is_empty() {
            return None;
        }
        dated.sort_by_key(|(date, v)| (*date, v.key()));
        let end = self.end_if_placed(next, ledger)?;
        let (date, v) = *dated.iter().find(|(date, _)| end > *date)?;
        Some((v, date, (end - date).num_days()))
    }

    /// The nodes with a hard date, with the date, and everything they wait on
    pub fn hard_chains(&self) -> Vec<(Vertex, NaiveDate, HashSet<Vertex>)> {
        let mut dated: Vec<_> = self.hard_dates().iter().map(|(v, d)| (*v, *d)).collect();
        dated.sort_by_key(|(v, _)| v.key());
        dated
            .into_iter()
            .map(|(v, date)| (v, date, self.net.ancestors([v])))
            .collect()
    }

    /// How late the plan's targets get through this node, the worst of them, and how many it
    /// leads to, `None` for a node that leads to none. Priorities count as days late.
    pub fn urgency(&self, v: &Vertex, slip: i64) -> Option<(i64, usize)> {
        let Vertex::Node(node) = v else {
            return None;
        };
        let plan_priority = i64::from(self.plan.scheduler_data().priority());
        let leads = self.baseline?.by_target.iter().filter_map(|t| {
            let slack = t.slack.get(node)?;
            let bump = self.bumps.get(&t.id).copied().unwrap_or(0);
            Some(slip - slack + plan_priority + i64::from(t.priority) + bump)
        });
        let leads: Vec<i64> = leads.collect();
        Some((*leads.iter().max()?, leads.len()))
    }

    /// Where a node feeding no target goes: higher priority plans first, then the least slack
    pub fn rest_rank(&self, v: &Vertex) -> (i64, i64) {
        let slack = match (self.baseline, v) {
            (Some(b), Vertex::Node(node)) => b.plan_slack.get(node).copied().unwrap_or(i64::MAX),
            _ => i64::MAX,
        };
        (-i64::from(self.plan.scheduler_data().priority()), slack)
    }

    // }}}

    // result {{{
    /// Pairs of tasks where one person works the second after the first and not alongside it,
    /// following each person's work in order
    fn resource_order(&self) -> Vec<(Vertex, Vertex)> {
        let mut by_person: HashMap<Uuid, Vec<(NaiveDate, NaiveDate, Vertex)>> = HashMap::new();
        for (v, placement) in &self.placed {
            let assignments = placement.work.iter().flat_map(|w| &w.assignments);
            for (who, days) in assignments {
                let (Assignee::User(user), Some(first), Some(last)) =
                    (who, days.keys().next(), days.keys().next_back())
                else {
                    continue;
                };
                by_person
                    .entry(*user)
                    .or_default()
                    .push((*first, *last, *v));
            }
        }
        let mut pairs = Vec::new();
        for work in by_person.values_mut() {
            work.sort_by_key(|(first, _, v)| (*first, v.key()));
            let follows = work.windows(2).filter(|w| w[0].1 < w[1].0);
            pairs.extend(follows.map(|w| (w[0].2, w[1].2)));
        }
        pairs
    }

    /// Open tasks and milestones with a date they must not start after, and that date
    fn hard_dates(&self) -> &HashMap<Vertex, NaiveDate> {
        &self.dated
    }

    fn find_hard_dates(&self) -> HashMap<Vertex, NaiveDate> {
        let nodes = self.order.iter();
        let hard =
            nodes.filter(|v| self.is_open(v) || matches!(v, Vertex::Node(PlanNode::Milestone(_))));
        let dated = hard.filter_map(|v| match self.constraint_of(v)? {
            TimeConstraint::Fixed(date) | TimeConstraint::Latest(date) => Some((*v, date)),
            _ => None,
        });
        dated.collect()
    }

    /// Days each visible node can finish later when `late` says the latest ends of some, see
    /// `Baseline::slack`
    fn slack_to(
        &self,
        ranges: &HashMap<Vertex, DateRange>,
        resource: &[(Vertex, Vertex)],
        late: HashMap<Vertex, NaiveDate>,
    ) -> HashMap<PlanNode, i64> {
        let late = late_ends(&self.net, ranges, resource, late);
        late.into_iter()
            .filter_map(|(v, end)| match v {
                Vertex::Node(node) if self.visible(&node) => {
                    Some((node, (end - ranges[&v].end()).num_days()))
                }
                _ => None,
            })
            .collect()
    }

    /// Slack against the end of the plan, of each target and of hard dates, for the solo schedule
    fn measure(&self) -> Baseline {
        let ranges: HashMap<Vertex, DateRange> =
            self.placed.iter().map(|(v, p)| (*v, p.range)).collect();
        let resource = self.resource_order();
        let plan_end = ranges.values().map(DateRange::end).max();
        let everything: HashMap<Vertex, NaiveDate> = ranges
            .keys()
            .filter_map(|v| Some((*v, plan_end?)))
            .collect();
        let plan_slack = self.slack_to(&ranges, &resource, everything.clone());
        let mut by_target = Vec::new();
        let mut goals: HashMap<Vertex, NaiveDate> = HashMap::new();
        for (id, target) in self.plan.targets() {
            let node = Vertex::Node(target.node());
            let Some(range) = ranges.get(&node) else {
                continue;
            };
            let late = HashMap::from([(node, range.end())]);
            goals.insert(node, range.end());
            by_target.push(TargetSlack {
                id: Some(*id),
                priority: target.priority(),
                slack: self.slack_to(&ranges, &resource, late),
            });
        }
        by_target.sort_by_key(|t| t.id);
        if by_target.is_empty() {
            let slack = plan_slack.clone();
            by_target.push(TargetSlack {
                id: None,
                priority: 0,
                slack,
            });
            goals = everything;
        }
        let mut late = goals;
        for (v, date) in self.hard_dates().iter().map(|(v, d)| (*v, *d)) {
            let Some(range) = ranges.get(&v) else {
                continue;
            };
            let length = (range.end() - range.start()).num_days().unsigned_abs();
            let limit = date + Days::new(length);
            let slot = late.entry(v).or_insert(limit);
            *slot = (*slot).min(limit);
        }
        Baseline {
            state: PlanVolatileState::default(),
            slack: self.slack_to(&ranges, &resource, late),
            by_target,
            plan_slack,
            ranges,
        }
    }

    fn constraint_of(&self, v: &Vertex) -> Option<TimeConstraint> {
        match v {
            Vertex::Node(PlanNode::Task(id)) => Some(self.plan.tasks()[id].time_constraint()),
            Vertex::Node(PlanNode::Milestone(id)) => {
                Some(self.plan.milestones()[id].time_constraint())
            }
            _ => None,
        }
    }

    /// The finished plan
    pub fn finish(self) -> PlanVolatileState {
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
            // the baseline is set by comparing with the plan on its own
            let forecast = TargetForecast {
                forecast: last,
                baseline: last,
            };
            out.targets.insert(*id, forecast);
        }
        out.plan_end = out.end().map(|end| {
            let last = end.pred_opt().expect("a day before");
            TargetForecast {
                forecast: last,
                baseline: last,
            }
        });
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

/// The people who may work a tag or group slot, in id order: plan users holding the tag, or
/// everyone in the group and the groups below it
pub fn eligible(tenant: &Tenant, plan: &Plan, worker: &Worker) -> Vec<Uuid> {
    let mut people: Vec<Uuid> = match worker {
        Worker::Tag(tag) => {
            let users = plan.users().iter();
            users
                .filter(|(_, u)| u.tags().contains(tag))
                .map(|(id, _)| *id)
                .collect()
        }
        Worker::UserGroup(group) => tenant.group_members(*group).into_iter().collect(),
        Worker::User(_) | Worker::Placeholder => Vec::new(),
    };
    people.sort();
    people
}
