// modules{{{
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::workload::Workload,
    common::{
        dates::{NodeEnd, TimeConstraint},
        short_id::ShortId,
    },
    planner::items::{
        dependency::PlanNode,
        milestone, plan_buffer, plan_delay,
        task::{self, Assignee},
        task_group,
    },
};
// }}}

/// Everything a scheduler run writes. It is derived from the tenant's state and rebuilt by the
/// next run, never saved or part of an update.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TenantVolatileState {
    pub plans: HashMap<ShortId, PlanVolatileState>,
    pub users: HashMap<Uuid, UserSchedule>,
}

// impl TenantVolatileState{{{
impl TenantVolatileState {
    /// The part of the schedule for a user who may see the given plans. Work on the others still
    /// takes people's hours, so it stays in their schedules with no plan or task named.
    pub fn visible_to(&self, plans: &HashSet<ShortId>) -> Self {
        let mut users = self.users.clone();
        for schedule in users.values_mut() {
            for assignment in schedule.assignments.values_mut().flatten() {
                if !plans.contains(&assignment.plan) {
                    assignment.plan = ShortId::default();
                    assignment.task = Uuid::nil();
                }
            }
        }
        Self {
            plans: self
                .plans
                .iter()
                .filter(|(id, _)| plans.contains(id))
                .map(|(id, plan)| (*id, plan.clone()))
                .collect(),
            users,
        }
    }

    /// Builds the result from each plan's, deriving every user's schedule across plans from the
    /// task assignments so the two cannot disagree
    pub fn from_plans(plans: HashMap<ShortId, PlanVolatileState>) -> Self {
        let mut users: HashMap<Uuid, UserSchedule> = HashMap::new();
        for (plan, state) in &plans {
            for (task, task_state) in &state.tasks {
                for (assignee, days) in &task_state.assignments {
                    let Assignee::User(user) = assignee else {
                        continue;
                    };
                    let schedule = users.entry(*user).or_default();
                    for (day, hours) in days {
                        let assignment = UserAssignment {
                            plan: *plan,
                            task: *task,
                            hours: *hours,
                        };
                        schedule
                            .assignments
                            .entry(*day)
                            .or_default()
                            .push(assignment);
                    }
                }
            }
        }
        for assignments in users.values_mut().flat_map(|u| u.assignments.values_mut()) {
            assignments.sort_by_key(|a| (a.plan.to_string(), a.task));
        }
        Self { plans, users }
    }
}
// }}}

// TenantVolatileState supporting types {{{
/// One plan's schedule. Items it could schedule are in the maps for their kind, the rest are in
/// `unscheduled` with the reason.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanVolatileState {
    pub tasks: HashMap<Uuid, task::VolatileState>,
    pub task_groups: HashMap<Uuid, task_group::VolatileState>,
    pub milestones: HashMap<Uuid, milestone::VolatileState>,
    pub plan_buffers: HashMap<Uuid, plan_buffer::VolatileState>,
    pub plan_delays: HashMap<Uuid, plan_delay::VolatileState>,
    pub unscheduled: HashMap<PlanNode, UnscheduledReason>,
    /// Forecast of each of the plan's targets
    pub targets: HashMap<Uuid, TargetForecast>,
    /// Forecast of the plan's last day, the target of a plan that has none of its own
    pub plan_end: Option<TargetForecast>,
    /// Tasks and milestones that landed on a date their time constraint does not allow
    pub constraint_violations: HashMap<PlanNode, ConstraintViolation>,
}

/// When a target lands as scheduled, against the earliest it could if nobody ever ran out of
/// hours
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetForecast {
    pub forecast: NaiveDate,
    pub baseline: NaiveDate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnscheduledReason {
    /// The task was dropped, so it is out of the plan
    Dropped,
    /// The workers the task asks for resolve to nobody, like a tag no one has or an empty group
    NoEligibleWorkers,
    /// Every dependency of the item could not be scheduled, so there is nothing to start it from
    DependsOnUnscheduled,
    /// A task group none of whose members could be scheduled
    NoScheduledMembers,
    /// The workers exist but cannot provide the allocation within the scheduling horizon, from a
    /// lack of availability or allowed days
    NoCapacity,
}

/// The constraint is not met, so the item was pushed to the closest date it could take
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstraintViolation {
    pub constraint: TimeConstraint,
    /// Whether the constraint and `scheduled` are about the first or the last day of the item
    pub end: NodeEnd,
    pub scheduled: NaiveDate,
    /// The earliest that day could be however free people are, if worked out. Being later than
    /// the date but no later than this is not something scheduling could have avoided.
    pub earliest: Option<NaiveDate>,
}

/// A person's work across every plan, to see what they are assigned on past, present and
/// upcoming days and how booked they are
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserSchedule {
    pub assignments: BTreeMap<NaiveDate, Vec<UserAssignment>>,
    /// Booked hours as a share of available hours from their first to last assigned day, set by
    /// the solver which knows availability. Near 1 or above marks a bottleneck.
    pub bottleneck: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserAssignment {
    pub plan: ShortId,
    pub task: Uuid,
    pub hours: Workload,
}
// }}}

// impl PlanVolatileState {{{
impl PlanVolatileState {
    /// The day after the last day anything in the plan is scheduled, `None` if nothing is
    pub fn end(&self) -> Option<NaiveDate> {
        let tasks = self.tasks.values().map(|t| t.date_range.end());
        let groups = self.task_groups.values().map(|g| g.date_range.end());
        let buffers = self.plan_buffers.values().map(|b| b.date_range.end());
        let milestones = self.milestones.values().map(|m| m.end());
        let delays = self.plan_delays.values().map(|d| d.date_range.end());
        tasks
            .chain(groups)
            .chain(buffers)
            .chain(delays)
            .chain(milestones)
            .max()
    }
}
// }}}

// impl TargetForecast {{{
impl TargetForecast {
    /// Days the target is later than the earliest it could be, never negative
    pub fn delay(&self) -> i64 {
        (self.forecast - self.baseline).num_days().max(0)
    }
}
// }}}

// impl ConstraintViolation {{{
impl ConstraintViolation {
    /// Days the node starts later than the date and than it had to
    pub fn avoidable(&self) -> i64 {
        let (TimeConstraint::Fixed(date) | TimeConstraint::Latest(date)) = self.constraint else {
            return 0;
        };
        let reachable = self.earliest.map_or(date, |e| e.max(date));
        (self.scheduled - reachable).num_days().max(0)
    }
}
// }}}

// impl UserSchedule {{{
impl UserSchedule {
    pub fn booked_on(&self, day: NaiveDate) -> u32 {
        let assignments = self.assignments.get(&day).into_iter().flatten();
        assignments.map(|a| u32::from(a.hours.get())).sum()
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_is_days_after_the_baseline_and_never_negative() {
        let d = |day| NaiveDate::from_ymd_opt(2024, 1, day).unwrap();
        let late = TargetForecast {
            forecast: d(9),
            baseline: d(5),
        };
        assert_eq!(late.delay(), 4);
        let early = TargetForecast {
            forecast: d(3),
            baseline: d(5),
        };
        assert_eq!(early.delay(), 0);
    }
}
// }}}
