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
    /// Tasks and milestones that landed on a date their time constraint does not allow
    pub constraint_violations: HashMap<PlanNode, ConstraintViolation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnscheduledReason {
    /// No chain of dependencies leads back to the plan start
    NoPathToPlanStart,
    /// The workers the task asks for resolve to nobody, like a tag no one has or an empty group
    NoEligibleWorkers,
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
