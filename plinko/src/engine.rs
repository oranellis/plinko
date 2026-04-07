use plinko_shared::data::Plan;
use plinko_shared::protocol::{
    PlanError, PlanRequest, PlanResponse, apply_milestone_patch, apply_task_patch,
};

pub struct PlanEngine {
    plan: Plan,
}

impl PlanEngine {
    pub fn new(plan: Plan) -> Self {
        Self { plan }
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn into_plan(self) -> Plan {
        self.plan
    }

    fn apply_validated<F>(&mut self, f: F) -> PlanResponse
    where
        F: FnOnce(&mut Plan) -> Result<(), PlanError>,
    {
        let backup = self.plan.clone();
        match f(&mut self.plan) {
            Err(e) => {
                self.plan = backup;
                PlanResponse::Error(e)
            }
            Ok(()) => match self.plan.compute_time_optimised_plan() {
                Ok(()) => PlanResponse::PlanUpdated,
                Err(e) => {
                    self.plan = backup;
                    PlanResponse::Error(PlanError::Scheduler(e))
                }
            },
        }
    }

    pub fn apply_request(&mut self, request: PlanRequest) -> PlanResponse {
        match request {
            PlanRequest::RunScheduler => match self.plan.compute_time_optimised_plan() {
                Ok(()) => PlanResponse::PlanUpdated,
                Err(e) => PlanResponse::Error(PlanError::Scheduler(e)),
            },
            PlanRequest::StartTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.start_task(id);
                    let _ = self.plan.compute_time_optimised_plan();
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }
            PlanRequest::PauseTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.pause_task(id);
                    let _ = self.plan.compute_time_optimised_plan();
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }
            PlanRequest::ResumeTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.resume_task(id);
                    let _ = self.plan.compute_time_optimised_plan();
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }
            PlanRequest::CompleteTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.complete_task(id);
                    let _ = self.plan.compute_time_optimised_plan();
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }
            PlanRequest::DropTask(id) => {
                if self.plan.tasks.contains_key(&id) {
                    self.plan.drop_task(id);
                    let _ = self.plan.compute_time_optimised_plan();
                    PlanResponse::PlanUpdated
                } else {
                    PlanResponse::Error(PlanError::TaskNotFound(id))
                }
            }
            PlanRequest::CreateTask(task) => {
                self.plan.add_task(task);
                let _ = self.plan.compute_time_optimised_plan();
                PlanResponse::PlanUpdated
            }
            PlanRequest::UpdateTask(id, patch) => {
                self.apply_validated(|plan| apply_task_patch(plan, id, patch))
            }
            PlanRequest::DeleteTask(id) => self.apply_validated(|plan| {
                if plan.delete_task(id) {
                    Ok(())
                } else {
                    Err(PlanError::TaskNotFound(id))
                }
            }),
            PlanRequest::CreateMilestone(milestone) => {
                self.plan.add_milestone(milestone);
                let _ = self.plan.compute_time_optimised_plan();
                PlanResponse::PlanUpdated
            }
            PlanRequest::UpdateMilestone(id, patch) => {
                self.apply_validated(|plan| apply_milestone_patch(plan, id, patch))
            }
            PlanRequest::DeleteMilestone(id) => self.apply_validated(|plan| {
                if plan.delete_milestone(id) {
                    Ok(())
                } else {
                    Err(PlanError::MilestoneNotFound(id))
                }
            }),
            PlanRequest::CreateUser(user) => {
                self.plan.add_user(user);
                PlanResponse::PlanUpdated
            }
            PlanRequest::UpdateUser(id, patch) => self.apply_validated(|plan| {
                let user = plan
                    .users_data
                    .get_mut(&id)
                    .map(|ud| &mut ud.user)
                    .ok_or(PlanError::UserNotFound(id))?;
                if let Some(v) = patch.name {
                    user.name = v;
                }
                if let Some(v) = patch.tags {
                    user.tags = v;
                }
                plan.node_allocations.invalidate();
                Ok(())
            }),
            PlanRequest::DeleteUser(id) => self.apply_validated(|plan| {
                if plan.users_data.remove(&id).is_some() {
                    plan.user_calendar_overrides.remove(&id);
                    plan.node_allocations.invalidate();
                    Ok(())
                } else {
                    Err(PlanError::UserNotFound(id))
                }
            }),
            PlanRequest::SetUserSchedule(id, schedule) => self.apply_validated(|plan| {
                if !plan.users_data.contains_key(&id) {
                    return Err(PlanError::UserNotFound(id));
                }
                plan.set_user_schedule(id, schedule);
                Ok(())
            }),
            PlanRequest::ClearUserSchedule(id) => self.apply_validated(|plan| {
                if !plan.users_data.contains_key(&id) {
                    return Err(PlanError::UserNotFound(id));
                }
                plan.clear_user_schedule(&id);
                Ok(())
            }),
            PlanRequest::SetDefaultSchedule(schedule) => self.apply_validated(|plan| {
                plan.default_schedule = schedule;
                Ok(())
            }),
            PlanRequest::SetCalendarOverride(date, hours) => self.apply_validated(|plan| {
                plan.calendar.set(date, hours);
                Ok(())
            }),
            PlanRequest::ClearCalendarOverride(date) => self.apply_validated(|plan| {
                plan.calendar.remove(&date);
                Ok(())
            }),
            PlanRequest::SetUserCalendarOverride(user_id, date, hours) => {
                self.apply_validated(|plan| {
                    plan.user_calendar_overrides
                        .entry(user_id)
                        .or_default()
                        .set(date, hours);
                    Ok(())
                })
            }
            PlanRequest::ClearUserCalendarOverride(user_id, date) => self.apply_validated(|plan| {
                if let Some(cal) = plan.user_calendar_overrides.get_mut(&user_id) {
                    cal.remove(&date);
                }
                Ok(())
            }),
            PlanRequest::ReplacePlan(new_plan) => {
                self.plan = *new_plan;
                let _ = self.plan.compute_time_optimised_plan();
                PlanResponse::PlanUpdated
            }
            PlanRequest::AddTag(name) => {
                self.plan.add_tag(name);
                PlanResponse::PlanUpdated
            }
            PlanRequest::RenameTag(id, new_name) => {
                self.plan.rename_tag(&id, &new_name);
                PlanResponse::PlanUpdated
            }
            PlanRequest::DeleteTag(id) => {
                self.plan.remove_tag(&id);
                PlanResponse::PlanUpdated
            }
            PlanRequest::MoveTag(id, new_index) => {
                self.plan.move_tag(&id, new_index);
                PlanResponse::PlanUpdated
            }
            PlanRequest::UpdatePlanSettings {
                name,
                start_date,
                scheduler_target,
            } => {
                self.plan.name = name;
                self.plan.start_date = start_date;
                self.plan.scheduler_target = scheduler_target;
                PlanResponse::PlanUpdated
            }
            // Server-level requests handled in server.rs
            PlanRequest::SavePlan
            | PlanRequest::NewPlan
            | PlanRequest::LoadPlan { .. }
            | PlanRequest::DeletePlan { .. }
            | PlanRequest::ListPlans
            | PlanRequest::SetCurrentUser(_) => PlanResponse::PlanUpdated,
        }
    }
}
