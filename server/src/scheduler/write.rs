// modules {{{
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::workload::Workload,
    common::dates::{DateRange, NodeConstraint, NodeEnd, TimeConstraint},
    multitenancy::tenant::Tenant,
    planner::{
        items::{
            dependency::PlanNode,
            milestone::{self, DerivedStatus},
            parent::Parent,
            plan_buffer, plan_delay,
            task::{self, Assignee},
            task_group,
        },
        plan::Plan,
        plan_entity::PlanItem,
    },
    scheduler::{
        arc::Work,
        day::Day,
        network::{Lowered, VertexId},
        pass::{Context, Pass, Placed},
        table::RowKey,
    },
    scheduler::{
        state::{
            ConstraintViolation, PlanVolatileState, TargetForecast, TenantVolatileState,
            UnscheduledReason,
        },
        status::milestone_statuses,
    },
};
// }}}

// write {{{
/// The schedule a finished pass gives, as dates on the tenant's own nodes
pub fn write(ctx: &Context, tenant: &Tenant, earliest: &Pass, pass: &Pass) -> TenantVolatileState {
    let mut plans = HashMap::new();
    for (index, info) in ctx.net.plans.iter().enumerate() {
        let plan = &tenant.plans()[&info.id];
        let statuses = milestone_statuses(plan);
        let open = statuses
            .iter()
            .filter(|(_, s)| **s != DerivedStatus::Completed);
        let lowered = |id: &Uuid| {
            ctx.net
                .lowered
                .get(&(index as u32, PlanNode::Milestone(*id)))
        };
        let pending = open
            .filter_map(|(id, _)| lowered(id))
            .map(|l| l.end)
            .collect();
        let writer = Writer {
            ctx,
            plan,
            index: index as u32,
            earliest,
            pass,
            statuses,
            pending,
        };
        plans.insert(info.id, writer.plan());
    }
    let mut state = TenantVolatileState::from_plans(plans);
    for (user, schedule) in &mut state.users {
        if let Some(row) = ctx.table.row(&RowKey::User(*user)) {
            schedule.bottleneck = pass.ledger.load(ctx.table, row);
        }
    }
    state
}
// }}}

/// One plan's part of a pass being written
struct Writer<'a> {
    ctx: &'a Context<'a>,
    plan: &'a Plan,
    index: u32,
    earliest: &'a Pass,
    pass: &'a Pass,
    statuses: HashMap<Uuid, DerivedStatus>,
    /// The vertices of milestones that have not happened
    pending: HashSet<VertexId>,
}

// impl Writer {{{
impl Writer<'_> {
    fn date(&self, day: Day) -> NaiveDate {
        day.date(self.ctx.table.origin)
    }

    /// The day before a vertex, which is the day what ends on it is shown on
    fn day_before(&self, day: Day) -> NaiveDate {
        self.date(Day(day.0.saturating_sub(1)))
    }

    fn lowered(&self, node: PlanNode) -> Option<Lowered> {
        self.ctx.net.lowered.get(&(self.index, node)).copied()
    }

    fn at(&self, pass: &Pass, v: VertexId) -> Option<Day> {
        pass.time[v.0 as usize]
    }

    /// The day what ends on a vertex is shown on: the day before it, but never before today
    /// unless work that happened decides it, then the last day of that work
    fn shown(&self, pass: &Pass, v: VertexId) -> Option<NaiveDate> {
        let (at, today) = (self.at(pass, v)?, self.ctx.table.today);
        let end = match at > today {
            true => Some(at),
            false => self.happened(pass, v),
        };
        Some(end.map_or(self.date(today), |end| self.day_before(end)))
    }

    /// The end of the latest work that happened before a vertex, through what it waits on. The
    /// start of today is not work, and a milestone that has not happened is today at the soonest.
    fn happened(&self, pass: &Pass, v: VertexId) -> Option<Day> {
        let (mut stack, mut seen, mut last) = (vec![v], HashSet::new(), None);
        while let Some(v) = stack.pop() {
            if !seen.insert(v) {
                continue;
            }
            if self.pending.contains(&v) {
                last = last.max(Some(self.ctx.table.today.next()));
                continue;
            }
            let vertex = self.ctx.net.vertex(v);
            let ends = vertex
                .incoming
                .iter()
                .filter_map(|a| pass.placed[a.0 as usize].as_ref());
            last = last.max(ends.map(|p| p.end).max());
            stack.extend(vertex.before());
        }
        last
    }

    fn placed<'p>(&self, pass: &'p Pass, node: PlanNode) -> Option<&'p Placed> {
        pass.placed[self.lowered(node)?.arc?.0 as usize].as_ref()
    }

    fn range(&self, placed: &Placed) -> Option<DateRange> {
        DateRange::new(self.date(placed.start), self.date(placed.end))
    }

    /// Why a node with an arc has no place
    fn reason(&self, node: PlanNode) -> UnscheduledReason {
        let known = self.ctx.net.unscheduled.get(&(self.index, node)).copied();
        let arc = self.lowered(node).and_then(|l| l.arc);
        let failed = arc.and_then(|a| self.pass.failed[a.0 as usize]);
        known
            .or(failed)
            .unwrap_or(UnscheduledReason::DependsOnUnscheduled)
    }

    fn plan(&self) -> PlanVolatileState {
        let mut out = PlanVolatileState::default();
        let plan = self.plan;
        for id in plan.tasks().keys() {
            let node = PlanNode::Task(*id);
            match self.task(node) {
                Some(state) => {
                    let range = state.date_range;
                    out.tasks.insert(*id, state);
                    let task = &plan.tasks()[id];
                    let work = self.work(node).filter(|w| w.is_open());
                    if work.is_some() {
                        self.violation(&mut out, node, task.time_constraint(), range);
                    }
                }
                None => drop(out.unscheduled.insert(node, self.reason(node))),
            }
        }
        for (id, item) in plan.milestones() {
            let node = PlanNode::Milestone(*id);
            let shown = self
                .lowered(node)
                .and_then(|l| self.shown(self.pass, l.end));
            match shown {
                Some(date) => {
                    let state = milestone::VolatileState {
                        derived_status: self.statuses[id],
                        date,
                    };
                    let range = DateRange::new(date, state.end());
                    out.milestones.insert(*id, state);
                    let constraint = NodeConstraint {
                        constraint: item.time_constraint(),
                        end: NodeEnd::End,
                    };
                    if let Some(range) = range {
                        self.violation(&mut out, node, constraint, range);
                    }
                }
                None => drop(out.unscheduled.insert(node, self.reason(node))),
            }
        }
        for id in plan.plan_buffers().keys() {
            let node = PlanNode::PlanBuffer(*id);
            match self.placed(self.pass, node).and_then(|p| self.range(p)) {
                Some(date_range) => {
                    let state = plan_buffer::VolatileState { date_range };
                    out.plan_buffers.insert(*id, state);
                }
                None => drop(out.unscheduled.insert(node, self.reason(node))),
            }
        }
        for id in plan.plan_delays().keys() {
            let node = PlanNode::PlanDelay(*id);
            match self.placed(self.pass, node).and_then(|p| self.range(p)) {
                Some(date_range) => {
                    let state = plan_delay::VolatileState { date_range };
                    out.plan_delays.insert(*id, state);
                }
                None => drop(out.unscheduled.insert(node, self.reason(node))),
            }
        }
        self.groups(&mut out);
        self.forecasts(&mut out);
        out
    }

    fn work(&self, node: PlanNode) -> Option<&Work> {
        self.ctx.net.arc(self.lowered(node)?.arc?).work()
    }

    fn task(&self, node: PlanNode) -> Option<task::VolatileState> {
        let (work, placed) = (self.work(node)?, self.placed(self.pass, node)?);
        let mut state = task::VolatileState {
            date_range: self.range(placed)?,
            assignments: HashMap::new(),
            workers: HashMap::new(),
        };
        for (slot, row) in work.slots.iter().zip(&placed.rows) {
            let who = match self.ctx.table.key(*row) {
                RowKey::User(user) => Assignee::User(user),
                RowKey::Plan(_) | RowKey::Full => Assignee::Placeholder,
            };
            state.workers.insert(slot.worker, who);
            if slot.hours == 0 {
                continue;
            }
            let days = state.assignments.entry(who).or_default();
            for day in &placed.days {
                let hours = days.entry(self.date(*day)).or_insert(Workload::MIN);
                *hours = Workload::new_const((hours.get() + slot.hours).min(8));
            }
        }
        Some(state)
    }

    /// Records a violation if a hard date is passed by the day of the node it is about, the
    /// first or the last of `range`
    fn violation(
        &self,
        out: &mut PlanVolatileState,
        node: PlanNode,
        constraint: NodeConstraint,
        range: DateRange,
    ) {
        let NodeConstraint { constraint, end } = constraint;
        let (TimeConstraint::Fixed(date) | TimeConstraint::Latest(date)) = constraint else {
            return;
        };
        let last = |day: NaiveDate| day.pred_opt().unwrap_or(day);
        let scheduled = match end {
            NodeEnd::Start => range.start(),
            NodeEnd::End => last(range.end()).max(range.start()),
        };
        if scheduled <= date {
            return;
        }
        // the soonest it could have been with everyone free, by the same reading of its day
        let soonest = match (node, end) {
            (PlanNode::Milestone(_), _) => {
                let at = self
                    .lowered(node)
                    .and_then(|l| self.at(self.earliest, l.end));
                at.map(|at| self.day_before(at))
            }
            (_, NodeEnd::Start) => self.placed(self.earliest, node).map(|p| self.date(p.start)),
            (_, NodeEnd::End) => self
                .placed(self.earliest, node)
                .map(|p| self.day_before(p.end)),
        };
        let violation = ConstraintViolation {
            constraint,
            end,
            scheduled,
            earliest: soonest,
        };
        out.constraint_violations.insert(node, violation);
    }

    /// Groups cover their members, so they go once everything else has its range, the ones
    /// inside others first
    fn groups(&self, out: &mut PlanVolatileState) {
        let mut members: HashMap<Uuid, Vec<PlanNode>> = HashMap::new();
        collect::<task::Task>(self.plan, &mut members);
        collect::<milestone::Milestone>(self.plan, &mut members);
        collect::<plan_buffer::PlanBuffer>(self.plan, &mut members);
        collect::<plan_delay::PlanDelay>(self.plan, &mut members);
        collect::<task_group::TaskGroup>(self.plan, &mut members);
        let mut left: Vec<Uuid> = self.plan.task_groups().keys().copied().collect();
        left.sort();
        while !left.is_empty() {
            let inner = |id: &Uuid| {
                let inside = members.get(id).into_iter().flatten();
                let mut groups = inside.filter_map(|n| match n {
                    PlanNode::TaskGroup(g) => Some(g),
                    _ => None,
                });
                groups.all(|g| !left.contains(g))
            };
            let Some(id) = left.iter().copied().find(inner) else {
                break;
            };
            left.retain(|g| *g != id);
            let node = PlanNode::TaskGroup(id);
            let inside = members.get(&id).map_or(&[][..], Vec::as_slice);
            let ranges: Vec<DateRange> = inside.iter().filter_map(|n| range_of(out, n)).collect();
            let empty = self.placed(self.pass, node).and_then(|p| self.range(p));
            let first = ranges.iter().map(DateRange::start).min();
            let last = ranges.iter().map(DateRange::end).max();
            let covered = first.zip(last).and_then(|(a, b)| DateRange::new(a, b));
            match empty.or(covered) {
                Some(date_range) => {
                    let state = task_group::VolatileState { date_range };
                    out.task_groups.insert(id, state);
                    let constraint = self.plan.task_groups()[&id].time_constraint();
                    self.violation(out, node, constraint, date_range);
                }
                None => {
                    let start = self.lowered(node).and_then(|l| self.at(self.pass, l.start));
                    let reason = match start {
                        Some(_) => UnscheduledReason::NoScheduledMembers,
                        None => UnscheduledReason::DependsOnUnscheduled,
                    };
                    out.unscheduled.insert(node, reason);
                }
            }
        }
    }

    /// When each target and the plan itself land, against the earliest they could
    fn forecasts(&self, out: &mut PlanVolatileState) {
        let info = &self.ctx.net.plans[self.index as usize];
        let forecast = |v: VertexId| {
            Some(TargetForecast {
                forecast: self.day_before(self.at(self.pass, v)?),
                baseline: self.day_before(self.at(self.earliest, v)?),
            })
        };
        for target in &info.targets {
            out.targets
                .extend(forecast(target.vertex).map(|f| (target.id, f)));
        }
        out.plan_end = forecast(info.end);
    }
}
// }}}

// helpers {{{
fn collect<T: PlanItem>(plan: &Plan, members: &mut HashMap<Uuid, Vec<PlanNode>>) {
    for (id, item) in T::entities(plan) {
        if let Parent::Node(group) = crate::planner::plan_entity::AbstractPlanNode::parent(item) {
            members.entry(group).or_default().push(T::node(*id));
        }
    }
}

/// The days a scheduled node is shown over, a milestone as its one day
fn range_of(out: &PlanVolatileState, node: &PlanNode) -> Option<DateRange> {
    match node {
        PlanNode::Task(id) => Some(out.tasks.get(id)?.date_range),
        PlanNode::Milestone(id) => {
            let milestone = out.milestones.get(id)?;
            DateRange::new(milestone.date, milestone.end())
        }
        PlanNode::PlanBuffer(id) => Some(out.plan_buffers.get(id)?.date_range),
        PlanNode::PlanDelay(id) => Some(out.plan_delays.get(id)?.date_range),
        PlanNode::TaskGroup(id) => Some(out.task_groups.get(id)?.date_range),
    }
}
// }}}
