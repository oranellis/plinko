// modules {{{
use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::short_id::ShortId,
    planner::items::{dependency::PlanNode, task::Worker},
    scheduler::{
        Hints,
        day::Day,
        network::Network,
        pass::{Context, Overrides, Pass},
        table::{RowKey, Table},
    },
};
// }}}

/// What a search decided, which is all of a schedule that cannot be worked out again without
/// searching: where each task that takes people's hours starts and who works it. The rest of
/// the schedule follows from it and the tenant, see `scheduler::expand`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Solution {
    pub tasks: BTreeMap<TaskKey, Placing>,
    /// What the next search starts from
    pub hints: Hints,
}

// Solution supporting types {{{
/// A task of a plan
pub type TaskKey = (ShortId, Uuid);

/// Where a task was put
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placing {
    pub start: NaiveDate,
    /// The person for each worker the task asks for that is a person
    pub people: Vec<(Worker, Uuid)>,
}

/// A solution as stored: whole, or as what changed since the one before it
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Stored {
    Full(Solution),
    Changes {
        placed: Vec<(TaskKey, Placing)>,
        removed: Vec<TaskKey>,
        hints: Hints,
    },
}
// }}}

// impl Solution {{{
impl Solution {
    /// What a finished pass decided
    pub fn of(ctx: &Context, pass: &Pass, hints: Hints) -> Self {
        let (net, table) = (ctx.net, ctx.table);
        let mut tasks = BTreeMap::new();
        for id in net.arc_ids().filter(|id| ctx.contended(*id)) {
            let arc = net.arc(id);
            let placed = pass.placed[id.0 as usize].as_ref();
            let (Some(work), Some(placed)) = (arc.work(), placed) else {
                continue;
            };
            let people = work.slots.iter().zip(&placed.rows);
            let people = people.filter_map(|(slot, row)| match table.key(*row) {
                RowKey::User(user) => Some((slot.worker, user)),
                _ => None,
            });
            let placing = Placing {
                start: placed.start.date(table.origin),
                people: people.collect(),
            };
            let key = (net.plans[arc.plan as usize].id, arc.node.id());
            tasks.insert(key, placing);
        }
        Self { tasks, hints }
    }

    /// The overrides that make a pass put every task where the solution has it
    pub fn overrides(&self, net: &Network, table: &Table) -> Overrides {
        let mut overrides = Overrides::default();
        for ((plan, task), placing) in &self.tasks {
            let index = net.plans.iter().position(|p| p.id == *plan);
            let lowered = index.and_then(|i| net.lowered.get(&(i as u32, PlanNode::Task(*task))));
            let Some(arc) = lowered.and_then(|l| l.arc) else {
                continue;
            };
            let start = Day::of(placing.start, table.origin);
            overrides.starts.insert(arc, start);
            let slots = net.arc(arc).work().map_or(&[][..], |w| &w.slots);
            for (worker, user) in &placing.people {
                let slot = slots.iter().position(|s| s.worker == *worker);
                let row = table.row(&RowKey::User(*user));
                overrides
                    .people
                    .extend(slot.zip(row).map(|(s, row)| ((arc, s), row)));
            }
        }
        overrides
    }

    /// How to store the solution after `previous`: as what changed, unless that is most of it
    pub fn stored_after(&self, previous: Option<&Solution>) -> Stored {
        let Some(previous) = previous else {
            return Stored::Full(self.clone());
        };
        let changed =
            |(key, placing): &(&TaskKey, &Placing)| previous.tasks.get(key) != Some(placing);
        let placed: Vec<(TaskKey, Placing)> = self
            .tasks
            .iter()
            .filter(changed)
            .map(|(key, placing)| (*key, placing.clone()))
            .collect();
        let gone = previous
            .tasks
            .keys()
            .filter(|key| !self.tasks.contains_key(key));
        let removed: Vec<TaskKey> = gone.copied().collect();
        if (placed.len() + removed.len()) * 2 > self.tasks.len() {
            return Stored::Full(self.clone());
        }
        Stored::Changes {
            placed,
            removed,
            hints: self.hints.clone(),
        }
    }

    /// The solution a stored record gives after this one
    pub fn then(mut self, stored: Stored) -> Solution {
        match stored {
            Stored::Full(solution) => solution,
            Stored::Changes {
                placed,
                removed,
                hints,
            } => {
                for key in removed {
                    self.tasks.remove(&key);
                }
                self.tasks.extend(placed);
                self.hints = hints;
                self
            }
        }
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    fn solution(starts: &[(u128, u32)]) -> Solution {
        let plan = ShortId::default();
        let tasks = starts.iter().map(|(task, day)| {
            let placing = Placing {
                start: NaiveDate::from_ymd_opt(2024, 1, *day).unwrap(),
                people: Vec::new(),
            };
            ((plan, Uuid::from_u128(*task)), placing)
        });
        Solution {
            tasks: tasks.collect(),
            hints: Hints::default(),
        }
    }

    #[test]
    fn a_solution_is_stored_as_what_changed_and_read_back_the_same() {
        let before = solution(&[(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (7, 7), (8, 8)]);
        let after = solution(&[(1, 1), (2, 9), (3, 3), (4, 4), (6, 6), (7, 7), (8, 8)]);
        let stored = after.stored_after(Some(&before));
        let Stored::Changes {
            placed, removed, ..
        } = &stored
        else {
            panic!("a task moved, one came and one went, which is not most of seven");
        };
        assert_eq!((placed.len(), removed.len()), (2, 1));
        assert_eq!(before.clone().then(stored), after);
        // the first of all, and one that changes most of what there is, are stored whole
        assert_eq!(after.stored_after(None), Stored::Full(after.clone()));
        let other = solution(&[(7, 1), (8, 2), (9, 3)]);
        assert_eq!(
            other.stored_after(Some(&before)),
            Stored::Full(other.clone())
        );
        assert_eq!(before.then(Stored::Full(other.clone())), other);
    }
}
// }}}
