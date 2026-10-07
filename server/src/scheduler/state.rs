// modules{{{
use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::types::{
    common::TimeConstraint,
    dependency::PlanNode,
    milestone, plan_buffer,
    short_id::ShortId,
    task::{self, Assignee},
    task_group,
    workload::Workload,
};
// }}}

/// Everything a scheduler run writes. It is derived from the tenant's state and rebuilt by the
/// next run, never saved or part of an update.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TenantVolatileState {
    pub plans: HashMap<ShortId, PlanVolatileState>,
    pub users: HashMap<Uuid, UserSchedule>,
}

// impl TenantVolatileState{{{
impl TenantVolatileState {
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

/// One plan's schedule. Items it could schedule are in the maps for their kind, the rest are in
/// `unscheduled` with the reason.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanVolatileState {
    pub tasks: HashMap<Uuid, task::VolatileState>,
    pub task_groups: HashMap<Uuid, task_group::VolatileState>,
    pub milestones: HashMap<Uuid, milestone::VolatileState>,
    pub plan_buffers: HashMap<Uuid, plan_buffer::VolatileState>,
    pub unscheduled: HashMap<PlanNode, UnscheduledReason>,
    /// Forecast of each of the plan's targets
    pub targets: HashMap<Uuid, TargetForecast>,
    /// Tasks and milestones that landed on a date their time constraint does not allow
    pub constraint_violations: HashMap<PlanNode, ConstraintViolation>,
}

/// When a target lands as scheduled, against when its plan would land it with no other plans
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetForecast {
    pub forecast: NaiveDate,
    pub baseline: NaiveDate,
}

impl TargetForecast {
    /// Days the target is later than the plan alone, never negative
    pub fn delay(&self) -> i64 {
        (self.forecast - self.baseline).num_days().max(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnscheduledReason {
    /// The task was dropped, so it is out of the plan
    Dropped,
    /// No chain of dependencies leads back to the plan start
    NoPathToPlanStart,
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConstraintViolation {
    pub constraint: TimeConstraint,
    pub scheduled: NaiveDate,
}

/// A person's work across every plan, to see what they are assigned on past, present and
/// upcoming days and how booked they are
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserSchedule {
    pub assignments: BTreeMap<NaiveDate, Vec<UserAssignment>>,
    /// Booked hours as a share of available hours from their first to last assigned day, set by
    /// the solver which knows availability. Near 1 or above marks a bottleneck.
    pub bottleneck: f32,
}

impl UserSchedule {
    pub fn booked_on(&self, day: NaiveDate) -> u32 {
        let assignments = self.assignments.get(&day).into_iter().flatten();
        assignments.map(|a| u32::from(a.hours.get())).sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UserAssignment {
    pub plan: ShortId,
    pub task: Uuid,
    pub hours: Workload,
}

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
