// modules {{{
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap},
};

use crate::{
    scheduler::state::UnscheduledReason,
    scheduler::{
        arc::{ArcId, ArcKind, Buffer, Progress, Work},
        bits::Valid,
        day::Day,
        ledger::Ledger,
        network::{Network, VertexId},
        objective::Late,
        table::{Ask, RowId, Table},
    },
};
// }}}

// constants {{{
type Reason = UnscheduledReason;

/// What decides which ready arc goes next, lowest first
pub type Key = [i64; 5];

/// Where a run of days starts and ends, and the row and day of other work that pushed it later
type Fit = (Day, Day, Option<(RowId, Day)>);

/// How many snapshots a pass leaves behind for later passes to start from
const SNAPSHOTS: usize = 8;
// }}}

/// What every pass over a tenant reads and none changes
pub struct Context<'a> {
    pub net: &'a Network,
    pub table: &'a Table,
    /// The asks and valid days of work whose people are all known up front
    fixed: Vec<Option<(Vec<Ask>, Valid)>>,
    /// The latest each arc may start for hard dates, for targets and for its plan's end, in that
    /// order, `i64::MAX` where nothing limits it. Empty until `bound` is worked out.
    pub latest: Vec<[i64; 3]>,
    /// How many days each arc could be worked on at all, fewer being harder to place
    pub room: Vec<i64>,
}

// Context supporting types {{{
/// How a pass picks what goes next and what it is told to keep
#[derive(Clone, Copy, Default)]
pub struct Rules<'a> {
    /// Place in order of these starts instead of by how urgent each arc is
    pub starts: Option<&'a [i64]>,
    /// Keep the people these placements chose
    pub people: Option<&'a [Option<Placed>]>,
    pub overrides: Option<&'a Overrides>,
    /// The latest each arc may start for hard dates, in place of the context's, for overrides
    /// that make dates more pressing
    pub hard: Option<&'a [i64]>,
    /// Nobody runs out of hours, which gives the earliest everything could be
    pub unlimited: bool,
}

/// Choices a pass is told to make differently
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// Arcs that wait until others are placed
    pub after: HashMap<ArcId, Vec<ArcId>>,
    /// The person for a slot of an arc
    pub people: HashMap<(ArcId, usize), RowId>,
    /// Arcs put on exactly these days, for a schedule that is already known
    pub starts: HashMap<ArcId, Day>,
    /// Hard dates counted as this many days sooner when deciding what goes first, for dates
    /// that were missed
    pub sooner: HashMap<Late, i64>,
}

/// Where an arc was put
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub start: Day,
    pub end: Day,
    /// The first day it could have started on
    pub ready: Day,
    /// The row of each slot
    pub rows: Vec<RowId>,
    /// The days it is worked on
    pub days: Vec<Day>,
    /// The arc whose hours kept it from starting sooner, and whose hours they were
    pub blocker: Option<(RowId, ArcId)>,
    /// A slot that another person could nearly as well have taken, and that person
    pub other: Option<(usize, RowId)>,
}
// }}}

// impl Context {{{
impl<'a> Context<'a> {
    pub fn new(net: &'a Network, table: &'a Table) -> Self {
        let fixed = net
            .arcs
            .iter()
            .map(|arc| {
                let work = arc.work()?;
                let rows: Option<Vec<RowId>> = work
                    .slots
                    .iter()
                    .map(|s| (s.candidates.len() == 1).then(|| s.candidates[0]))
                    .collect();
                let asks = asks(work, &rows?);
                let valid = table.valid(&asks, work.mask, work.calendar);
                Some((asks, valid))
            })
            .collect();
        Self {
            net,
            table,
            fixed,
            latest: Vec::new(),
            room: Vec::new(),
        }
    }

    /// The hours asked of each row and the days work can be done on with these people
    pub fn valid(
        &self,
        arc: ArcId,
        work: &Work,
        rows: &[RowId],
    ) -> (Cow<'_, [Ask]>, Cow<'_, Valid>) {
        match &self.fixed[arc.0 as usize] {
            Some((asks, valid)) => (Cow::Borrowed(asks), Cow::Borrowed(valid)),
            None => {
                let asks = asks(work, rows);
                let valid = self.table.valid(&asks, work.mask, work.calendar);
                (Cow::Owned(asks), Cow::Owned(valid))
            }
        }
    }

    /// Whether placing the arc takes hours people can run out of, so its place depends on what
    /// went before it
    pub fn contended(&self, arc: ArcId) -> bool {
        let Some(work) = self.net.arc(arc).work() else {
            return false;
        };
        let limited = |row: &RowId| self.table.claimable(*row);
        let mut slots = work.slots.iter().filter(|s| s.hours > 0);
        work.is_open() && slots.any(|s| s.candidates.iter().any(limited))
    }

    fn key(&self, arc: ArcId, rules: &Rules) -> Key {
        let i = arc.0 as usize;
        let [hard, target, end] = self.latest.get(i).copied().unwrap_or([i64::MAX; 3]);
        let room = self.room.get(i).copied().unwrap_or(0);
        let hard = rules.hard.map_or(hard, |hard| hard[i]);
        match rules.starts {
            Some(starts) => [starts[i], hard, target, end, room],
            None => [hard, target, end, room, 0],
        }
    }
}
// }}}

/// The hours asked of each row by the slots given these people, one ask for each row
pub fn asks(work: &Work, rows: &[RowId]) -> Vec<Ask> {
    let mut asks: Vec<Ask> = Vec::new();
    for (slot, row) in work.slots.iter().zip(rows).filter(|(s, _)| s.hours > 0) {
        match asks.iter_mut().find(|(r, _)| r == row) {
            Some(ask) => ask.1 += slot.hours,
            None => asks.push((*row, slot.hours)),
        }
    }
    asks
}

/// One schedule of the network being built: arcs are placed one at a time as early as they fit,
/// once everything they wait on is settled
#[derive(Clone)]
pub struct Pass {
    pub ledger: Ledger,
    /// The time of each vertex, `None` if it is not settled or nothing it waits on is scheduled
    pub time: Vec<Option<Day>>,
    settled: Vec<bool>,
    /// How many of the things each vertex waits on are not settled yet
    waiting: Vec<u32>,
    pub placed: Vec<Option<Placed>>,
    /// Arcs that are placed or never will be
    done: Vec<bool>,
    pub failed: Vec<Option<Reason>>,
    ready: BTreeSet<(Key, ArcId)>,
    /// Ready arcs held back by an override
    deferred: Vec<ArcId>,
    /// Work not yet placed that each row may be asked for, shared between who could do it
    demand: Vec<u64>,
    /// The arcs that took people's hours, in the order they were placed
    pub steps: Vec<ArcId>,
    /// Whether something did not fit before the table ended though it has days it could go on
    pub overflow: bool,
}

// impl Pass {{{
impl Pass {
    /// A pass with what has happened in place and everything that follows from that settled
    pub fn new(ctx: &Context, rules: &Rules) -> Self {
        let net = ctx.net;
        let mut pass = Self {
            ledger: Ledger::new(ctx.table),
            time: vec![None; net.vertices.len()],
            settled: vec![false; net.vertices.len()],
            waiting: net.vertices.iter().map(|v| v.inputs() as u32).collect(),
            placed: vec![None; net.arcs.len()],
            done: vec![false; net.arcs.len()],
            failed: vec![None; net.arcs.len()],
            ready: BTreeSet::new(),
            deferred: Vec::new(),
            demand: vec![0; ctx.table.rows()],
            steps: Vec::new(),
            overflow: false,
        };
        for arc in net.arc_ids().filter(|a| ctx.contended(*a)) {
            pass.want(ctx, arc, true);
        }
        let mut stack = Vec::new();
        // finished work before work in progress, as the first is where it is whatever happens
        for active in [false, true] {
            let mut history: Vec<ArcId> = net
                .arc_ids()
                .filter(|a| match net.arc(*a).work().map(|w| w.progress) {
                    Some(Progress::Done { .. }) => !active,
                    Some(Progress::Active { .. }) => active,
                    _ => false,
                })
                .collect();
            history.sort_by_key(|a| (ctx.key(*a, rules), *a));
            for arc in history {
                pass.settle_arc(ctx, rules, arc, &mut stack);
            }
        }
        let free = (0..net.vertices.len()).filter(|i| pass.waiting[*i] == 0);
        stack.extend(free.map(|i| VertexId(i as u32)));
        pass.settle(ctx, rules, stack);
        pass
    }

    /// Places every arc that is left. Snapshots are taken on the way if asked for, as the number
    /// of arcs placed so far and the pass as it was then.
    pub fn run(
        &mut self,
        ctx: &Context,
        rules: &Rules,
        mut snapshots: Option<&mut Vec<(usize, Pass)>>,
    ) {
        let every = (ctx.net.arcs.len() / SNAPSHOTS).max(1);
        while let Some(arc) = self.pick(ctx, rules) {
            if let Some(snapshots) = snapshots.as_deref_mut()
                && self.steps.len().is_multiple_of(every)
                && snapshots
                    .last()
                    .is_none_or(|(at, _)| *at < self.steps.len())
            {
                // the arc goes back so the snapshot can pick it again
                self.ready.insert((ctx.key(arc, rules), arc));
                snapshots.push((self.steps.len(), self.clone()));
                self.ready.remove(&(ctx.key(arc, rules), arc));
            }
            self.steps.push(arc);
            let mut stack = Vec::new();
            self.settle_arc(ctx, rules, arc, &mut stack);
            self.settle(ctx, rules, stack);
        }
    }

    /// The next arc to place: the first ready one that no override holds back. If only held back
    /// ones are left, the overrides cannot all be kept, so the first of those goes.
    fn pick(&mut self, ctx: &Context, rules: &Rules) -> Option<ArcId> {
        let held = |pass: &Self, arc: &ArcId| {
            let after = rules.overrides.and_then(|o| o.after.get(arc));
            after.is_some_and(|others| others.iter().any(|b| !pass.done[b.0 as usize]))
        };
        // what is no longer held back is ready like the rest
        let (free, still): (Vec<ArcId>, Vec<ArcId>) = std::mem::take(&mut self.deferred)
            .into_iter()
            .partition(|arc| !held(self, arc));
        self.deferred = still;
        let free = free.into_iter().map(|arc| (ctx.key(arc, rules), arc));
        self.ready.extend(free);
        while let Some((_, arc)) = self.ready.pop_first() {
            if !held(self, &arc) {
                return Some(arc);
            }
            self.deferred.push(arc);
        }
        let first = self.deferred.iter().copied().min()?;
        self.deferred.retain(|a| *a != first);
        Some(first)
    }

    /// Adds or takes away what an arc may ask of each row it could use
    fn want(&mut self, ctx: &Context, arc: ArcId, add: bool) {
        let Some(work) = ctx.net.arc(arc).work() else {
            return;
        };
        for slot in work.slots.iter().filter(|s| s.hours > 0) {
            let share = u64::from(work.days) * 1000 / slot.candidates.len().max(1) as u64;
            for row in &slot.candidates {
                let demand = &mut self.demand[row.0 as usize];
                *demand = match add {
                    true => *demand + share,
                    false => demand.saturating_sub(share),
                };
            }
        }
    }

    /// Settles vertices whose inputs are all settled, and through them everything that follows
    /// without taking people's hours. Arcs that do take them become ready.
    fn settle(&mut self, ctx: &Context, rules: &Rules, mut stack: Vec<VertexId>) {
        while let Some(v) = stack.pop() {
            let i = v.0 as usize;
            if std::mem::replace(&mut self.settled[i], true) {
                continue;
            }
            let vertex = ctx.net.vertex(v);
            self.time[i] = vertex.pinned.or_else(|| {
                let after = vertex.after.iter().map(|u| self.time[u.0 as usize]);
                let ends = |a: &ArcId| Some(self.placed[a.0 as usize].as_ref()?.end);
                let incoming = vertex.incoming.iter().map(ends);
                let waits = !vertex.after.is_empty() || !vertex.incoming.is_empty();
                // the group it is in has to have started, which alone does not schedule it
                let started = match vertex.anchor {
                    Some(anchor) => Some(self.time[anchor.0 as usize]?),
                    None => None,
                };
                match after.chain(incoming).flatten().max() {
                    None if waits => None,
                    waited => waited.max(started).max(vertex.earliest),
                }
            });
            for next in &vertex.next {
                self.arrive(*next, &mut stack);
            }
            for arc in &vertex.outgoing {
                if self.done[arc.0 as usize] {
                    continue;
                }
                if self.time[i].is_some() && ctx.contended(*arc) && !rules.unlimited {
                    self.ready.insert((ctx.key(*arc, rules), *arc));
                } else {
                    self.settle_arc(ctx, rules, *arc, &mut stack);
                }
            }
        }
    }

    /// One input of the vertex is settled
    fn arrive(&mut self, v: VertexId, stack: &mut Vec<VertexId>) {
        let waiting = &mut self.waiting[v.0 as usize];
        *waiting = waiting.saturating_sub(1);
        if *waiting == 0 {
            stack.push(v);
        }
    }

    /// Places the arc, or gives up on it, and tells its end vertex
    fn settle_arc(&mut self, ctx: &Context, rules: &Rules, arc: ArcId, stack: &mut Vec<VertexId>) {
        let i = arc.0 as usize;
        if std::mem::replace(&mut self.done[i], true) {
            return;
        }
        if ctx.contended(arc) {
            self.want(ctx, arc, false);
        }
        match self.place(ctx, rules, arc) {
            Ok(placed) => self.placed[i] = Some(placed),
            Err(reason) => self.failed[i] = Some(reason),
        }
        self.arrive(ctx.net.arc(arc).end, stack);
    }

    fn place(&mut self, ctx: &Context, rules: &Rules, id: ArcId) -> Result<Placed, Reason> {
        let arc = ctx.net.arc(id);
        let at = self.time[arc.start.0 as usize];
        match &arc.kind {
            ArcKind::Buffer(buffer) => {
                let start = at.ok_or(Reason::DependsOnUnscheduled)?;
                self.buffer(ctx, buffer, start)
            }
            ArcKind::Work(work) => match work.progress {
                Progress::Open => {
                    let from = at.ok_or(Reason::DependsOnUnscheduled)?;
                    let from = from.max(work.floor_start.unwrap_or(from));
                    self.open(ctx, rules, id, work, from)
                }
                Progress::Active { begun } => self.active(ctx, rules, id, work, begun),
                // finished work holds no hours, so its people are free from today on
                Progress::Done { begun, ended } => {
                    let rows = self.people(ctx, rules, id, work, begun, &mut None)?;
                    Ok(placed(begun, ended, rows, Vec::new()))
                }
            },
        }
    }

    fn buffer(&mut self, ctx: &Context, buffer: &Buffer, start: Day) -> Result<Placed, Reason> {
        let working = ctx.table.valid(&[], None, buffer.calendar);
        let after = |days: u32| match days {
            0 => Some(start),
            _ => working.nth_from(start, days).map(Day::next),
        };
        match buffer.end(start, after) {
            Some(end) => Ok(placed(start, end, Vec::new(), Vec::new())),
            None => Err(self.short(&working)),
        }
    }

    fn take(&mut self, rules: &Rules, asks: &[Ask], days: &[Day], arc: ArcId) {
        if !rules.unlimited {
            self.ledger.claim(asks, days, arc);
        }
    }

    /// The first day work can start on from `from`, which is later if it may not end before a
    /// day
    fn floor(work: &Work, valid: &Valid, from: Day) -> Day {
        let Some(not_before) = work.floor_end.filter(|_| work.days > 0) else {
            return from;
        };
        // its last day is the first it can be worked on from the day before that
        let last = valid.next(Day(not_before.0.saturating_sub(1)));
        let start = last.and_then(|last| valid.nth_back(last.next(), work.days));
        from.max(start.unwrap_or(from))
    }

    /// Where the run of days goes from `from` for these people, without taking anything, none
    /// if it does not fit before the table ends
    fn fit(
        &self,
        rules: &Rules,
        work: &Work,
        asks: &[Ask],
        valid: &Valid,
        from: Day,
        days: u32,
    ) -> Option<Fit> {
        let from = Self::floor(work, valid, from);
        let window = match rules.unlimited {
            true => self.ledger.window(valid, &[], from, days),
            false => self.ledger.window(valid, asks, from, days),
        };
        window.map(|w| (w.start, w.end, w.blocker))
    }

    /// Work did not fit, which more days might change if it has days it could go on at all
    fn short(&mut self, valid: &Valid) -> Reason {
        self.overflow |= valid.count() > 0;
        Reason::NoCapacity
    }

    /// The person for each slot. A slot with one candidate has that one. For the others it is
    /// whoever the rules name, or whoever ends the work soonest from `from`, the one less asked
    /// for by other work where two end together. Never someone already on the work.
    fn people(
        &mut self,
        ctx: &Context,
        rules: &Rules,
        id: ArcId,
        work: &Work,
        from: Day,
        other: &mut Option<(usize, RowId)>,
    ) -> Result<Vec<RowId>, Reason> {
        if let Some(kept) = rules.people.and_then(|p| p[id.0 as usize].as_ref()) {
            return Ok(kept.rows.clone());
        }
        let single = |i: &usize| work.slots[*i].candidates.len() == 1;
        let mut order: Vec<usize> = (0..work.slots.len()).collect();
        order.sort_by_key(|i| (work.slots[*i].candidates.len(), *i));
        let mut chosen: Vec<(usize, RowId)> = Vec::new();
        for i in order {
            let slot = &work.slots[i];
            let used =
                |row: &RowId| ctx.table.claimable(*row) && chosen.iter().any(|c| c.1 == *row);
            let named = rules.overrides.and_then(|o| o.people.get(&(id, i)));
            let row = match (
                single(&i),
                named.filter(|row| slot.candidates.contains(row)),
            ) {
                (true, _) => slot.candidates[0],
                (false, Some(row)) => *row,
                (false, None) => {
                    let free = slot.candidates.iter().filter(|row| !used(row));
                    let mut ends: Vec<(Day, u64, RowId)> = Vec::new();
                    // whether someone ran out of days that more days could give them
                    let mut short = false;
                    for row in free {
                        // finished work holds no hours, so anyone will do
                        if slot.hours == 0 || matches!(work.progress, Progress::Done { .. }) {
                            ends.push((from, 0, *row));
                            continue;
                        }
                        let mut trial: Vec<(usize, RowId)> = chosen.clone();
                        trial.push((i, *row));
                        trial.sort();
                        let slots: Vec<_> =
                            trial.iter().map(|(s, _)| work.slots[*s].clone()).collect();
                        let rows: Vec<RowId> = trial.iter().map(|(_, row)| *row).collect();
                        let partial = Work {
                            slots,
                            ..work.clone()
                        };
                        let asks = asks(&partial, &rows);
                        let valid = ctx.table.valid(&asks, work.mask, work.calendar);
                        match self.fit(rules, work, &asks, &valid, from, work.days) {
                            Some((_, end, _)) => {
                                ends.push((end, self.demand[row.0 as usize], *row))
                            }
                            None => short |= valid.count() > 0,
                        }
                    }
                    ends.sort();
                    if let [_, second, ..] = ends.as_slice() {
                        *other = Some((i, second.2));
                    }
                    let Some(first) = ends.first() else {
                        self.overflow |= short;
                        return Err(Reason::NoCapacity);
                    };
                    first.2
                }
            };
            chosen.push((i, row));
        }
        chosen.sort();
        Ok(chosen.into_iter().map(|(_, row)| row).collect())
    }

    fn open(
        &mut self,
        ctx: &Context,
        rules: &Rules,
        id: ArcId,
        work: &Work,
        from: Day,
    ) -> Result<Placed, Reason> {
        let known = rules.overrides.and_then(|o| o.starts.get(&id));
        let from = known.copied().unwrap_or(from);
        let mut other = None;
        let rows = self.people(ctx, rules, id, work, from, &mut other)?;
        let (asks, valid) = ctx.valid(id, work, &rows);
        if asks.iter().any(|(_, hours)| *hours > 8) {
            return Err(Reason::NoCapacity);
        }
        let ready = Self::floor(work, &valid, from);
        let Some((start, end, blocker)) = self.fit(rules, work, &asks, &valid, from, work.days)
        else {
            return Err(self.short(&valid));
        };
        let days: Vec<Day> = valid.days(start, end).collect();
        self.take(rules, &asks, &days, id);
        let holder = |(row, day)| Some((row, self.ledger.holder(row, day)?));
        Ok(Placed {
            ready,
            blocker: blocker.and_then(holder),
            other,
            ..placed(start, end, rows, days)
        })
    }

    /// Work in progress has its hours on every day it could be worked from when it began up to
    /// today, today too if that is free, and then what is left of its days in one run
    fn active(
        &mut self,
        ctx: &Context,
        rules: &Rules,
        id: ArcId,
        work: &Work,
        begun: Day,
    ) -> Result<Placed, Reason> {
        let today = ctx.table.today;
        let from = begun.max(today);
        let rows = self.people(ctx, rules, id, work, from, &mut None)?;
        let (asks, valid) = ctx.valid(id, work, &rows);
        let mut days: Vec<Day> = valid.days(begun, today).collect();
        let left = work.days.saturating_sub(days.len() as u32);
        let mut end = today.next();
        if left > 0 {
            let Some((start, to, _)) = self.fit(rules, work, &asks, &valid, from, left) else {
                return Err(self.short(&valid));
            };
            days.extend(valid.days(start, to));
            end = end.max(to);
        } else if let Some((start, to, _)) = self.fit(rules, work, &asks, &valid, today, 1)
            && start == today
        {
            days.extend(valid.days(start, to));
        }
        self.take(rules, &asks, &days, id);
        Ok(placed(begun, end, rows, days))
    }
}
// }}}

// helpers {{{
fn placed(start: Day, end: Day, rows: Vec<RowId>, days: Vec<Day>) -> Placed {
    Placed {
        start,
        end,
        ready: start,
        rows,
        days,
        blocker: None,
        other: None,
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use chrono::{Days, NaiveDate};
    use uuid::Uuid;

    use super::*;
    use crate::{
        availability::workload::Workload,
        planner::items::{dependency::PlanNode, task::Worker},
        scheduler::{
            arc::{Arc, Slot},
            network::Vertex,
            table::RowKey,
        },
    };

    /// A pass over one task that alice or bob can work, bob only for the first two of 30 days
    fn one_task(progress: Progress, days: u32) -> Pass {
        let origin = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let mut table = Table::new(origin, 30, origin);
        let person = |n| RowKey::User(Uuid::from_u128(n));
        let alice = table.insert(person(1), |_| Workload::MAX);
        let gone = origin + Days::new(2);
        let bob = table.insert(person(2), |d| match d < gone {
            true => Workload::MAX,
            false => Workload::MIN,
        });
        let mut net = Network::default();
        net.root = net.add_vertex(Vertex {
            pinned: Some(table.today),
            ..Vertex::default()
        });
        let end = net.add_vertex(Vertex::default());
        let mut work = Work::wait(days, table.full());
        work.progress = progress;
        work.slots = vec![Slot {
            worker: Worker::Placeholder,
            hours: 8,
            candidates: vec![alice, bob],
        }];
        let kind = ArcKind::Work(work);
        let (node, start) = (PlanNode::Task(Uuid::nil()), net.root);
        net.add_arc(Arc {
            node,
            plan: 0,
            start,
            end,
            kind,
        });
        net.link();
        let ctx = Context::new(&net, &table);
        let mut pass = Pass::new(&ctx, &Rules::default());
        pass.run(&ctx, &Rules::default(), None);
        pass
    }

    #[test]
    fn only_work_nobody_can_fit_asks_for_more_days() {
        // bob never fits, but alice does
        let pass = one_task(Progress::Open, 5);
        assert_eq!(pass.placed[0].as_ref().unwrap().rows, [RowId(1)]);
        assert!(!pass.overflow);
        let pass = one_task(Progress::Open, 40);
        assert!(pass.placed[0].is_none() && pass.overflow);
        // finished work holds no hours, so it has a person whatever its days
        let done = Progress::Done {
            begun: Day(0),
            ended: Day(40),
        };
        let pass = one_task(done, 40);
        assert!(pass.placed[0].is_some() && !pass.overflow);
    }
}
// }}}
