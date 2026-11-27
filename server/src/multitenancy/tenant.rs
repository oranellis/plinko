// modules{{{
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    availability::{calendar::Calendar, rule_set::RuleSet, workload::Workload},
    common::{
        crud::Crud,
        dates::TimeConstraint,
        error::{PlinkoError, PlinkoResult},
        short_id::ShortId,
    },
    multitenancy::{
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{Role, TenantLoginUser, TenantUserVolatileState},
        user_group::TenantUserGroup,
    },
    planner::{
        items::task::{Assignee, Status},
        plan::Plan,
        plan_update::PlanUpdate,
        resources::user::NewPlanUser,
    },
    scheduler::{self, Hints, state::TenantVolatileState},
    traits::entity::Entity,
};
// }}}

// constants {{{
/// How far before or after today a date in a plan may be
pub const MAX_DAYS_FROM_TODAY: i64 = 10 * 366;

/// The longest a task, buffer or delay may last, in days
pub const MAX_DURATION: u32 = 10 * 366;
// }}}

/// Everything a tenant is, and so what a snapshot holds. The caches and the schedule are
/// derived and left out, so a loaded tenant needs `scheduler::schedule` before it has them.
#[derive(Clone, Serialize, Deserialize)]
pub struct Tenant {
    id: ShortId,
    name: String,
    /// The current day, which only changes by update (the server runner sends one each day)
    today: NaiveDate,
    pub(super) users: HashMap<Uuid, TenantLoginUser>,
    pub(super) user_groups: HashMap<Uuid, TenantUserGroup>,
    #[serde(skip)]
    volatile: HashMap<Uuid, TenantUserVolatileState>,
    #[serde(skip)]
    plan_calendars: HashMap<ShortId, Calendar<Workload>>,
    #[serde(skip)]
    schedule: Option<TenantVolatileState>,
    /// What the last search found, for the next to start from
    #[serde(skip)]
    hints: Hints,
    pub(super) plans: HashMap<ShortId, Plan>,
    pub(super) default_schedule: RuleSet<Workload>,
}

// impl Tenant{{{
impl Tenant {
    pub fn new(id: ShortId, name: String, today: NaiveDate) -> Self {
        let root_user = TenantLoginUser::new(
            "Root User".to_string(),
            "root@localhost".to_string(),
            Role::Admin,
        );
        Self::new_with_login(id, name, today, root_user)
    }

    /// Create a new tenant with a specific initial login
    ///
    /// * `id`: A unique ID for the tenant
    /// * `name`: A friendly name for the tenant
    /// * `today`: The day it starts on
    /// * `user`: An initial user to populate the tenant with
    pub fn new_with_login(
        id: ShortId,
        name: String,
        today: NaiveDate,
        user: TenantLoginUser,
    ) -> Self {
        let mut users = HashMap::new();
        users.insert(*user.id(), user);
        Self {
            id,
            name,
            today,
            users,
            user_groups: HashMap::new(),
            volatile: HashMap::new(),
            plan_calendars: HashMap::new(),
            schedule: None,
            hints: Hints::default(),
            plans: HashMap::new(),
            default_schedule: RuleSet::default(),
        }
    }

    pub fn id(&self) -> &ShortId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn today(&self) -> NaiveDate {
        self.today
    }

    /// The availability a new user and a new plan start with
    pub fn default_schedule(&self) -> &RuleSet<Workload> {
        &self.default_schedule
    }

    pub fn users(&self) -> &HashMap<Uuid, TenantLoginUser> {
        &self.users
    }

    pub fn user_groups(&self) -> &HashMap<Uuid, TenantUserGroup> {
        &self.user_groups
    }

    pub fn plans(&self) -> &HashMap<ShortId, Plan> {
        &self.plans
    }

    fn insert_plan(&mut self, plan: Plan) -> PlinkoResult<()> {
        match self.plans.entry(*plan.id()) {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                Err(PlinkoError::PlanExists(*occupied_entry.get().id()))
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(plan);
                Ok(())
            }
        }
    }

    /// The last scheduler result, which any successful update clears until the next run
    pub fn schedule(&self) -> Option<&TenantVolatileState> {
        self.schedule.as_ref()
    }

    pub fn store_schedule(&mut self, schedule: TenantVolatileState) {
        self.schedule = Some(schedule);
    }

    pub fn store_hints(&mut self, hints: Hints) {
        self.hints = hints;
    }

    /// The choices the last search found better made the other way, which updates leave alone
    pub fn hints(&self) -> &Hints {
        &self.hints
    }

    pub fn apply_update(&mut self, u: TenantUpdate) -> PlinkoResult<()> {
        let result = self.apply_update_unscheduled(u);
        if result.is_ok() {
            self.schedule = None;
        }
        result
    }

    fn apply_update_unscheduled(&mut self, u: TenantUpdate) -> PlinkoResult<()> {
        let is_admin = self
            .users
            .get(&u.actor)
            .ok_or(PlinkoError::Forbidden(u.actor))?
            .role()
            == Role::Admin;
        match u.update_type {
            TenantUpdateType::User(update) => {
                let own_profile =
                    matches!(&update, Crud::Update(p) if p.id == u.actor && p.role.is_none());
                allow(is_admin || own_profile, u.actor)?;
                allow(
                    !matches!(&update, Crud::Remove(id) if *id == u.actor),
                    u.actor,
                )?;
                let (added, removed) = match &update {
                    Crud::Add(new) => (Some(new.id), None),
                    Crud::Remove(id) => (None, Some(*id)),
                    Crud::Update(_) => (None, None),
                };
                TenantLoginUser::apply_crud(self, update)?;
                if let Some(user) = added.and_then(|id| self.users.get_mut(&id)) {
                    user.availability = self.default_schedule.clone();
                }
                removed.map(|id| self.volatile.remove(&id));
                Ok(())
            }
            TenantUpdateType::UserGroup(update) => {
                allow(is_admin, u.actor)?;
                TenantUserGroup::apply_crud(self, update)
            }
            TenantUpdateType::Availability(user, update) => {
                allow(is_admin || user == u.actor, u.actor)?;
                self.users
                    .get_mut(&user)
                    .ok_or(PlinkoError::ItemDoesNotExist(user))?
                    .availability
                    .apply_update(update)?;
                self.volatile.remove(&user);
                Ok(())
            }
            TenantUpdateType::DefaultSchedule(update) => {
                allow(is_admin, u.actor)?;
                self.default_schedule.apply_update(update)
            }
            TenantUpdateType::Favourite(plan, starred) => {
                self.authorize_plan(u.actor, is_admin, &plan)?;
                let favourites = &mut self
                    .users
                    .get_mut(&u.actor)
                    .ok_or(PlinkoError::Forbidden(u.actor))?
                    .favourites;
                if starred {
                    favourites.insert(plan);
                } else {
                    favourites.remove(&plan);
                }
                Ok(())
            }
            TenantUpdateType::SetToday(day) => {
                allow(is_admin, u.actor)?;
                self.today = day;
                Ok(())
            }
            TenantUpdateType::Plan(update) => self.plan_update(u.actor, is_admin, update),
        }
    }

    fn plan_update(
        &mut self,
        actor: Uuid,
        is_admin: bool,
        update: PlanTenantUpdate,
    ) -> PlinkoResult<()> {
        match update {
            PlanTenantUpdate::Add(new) => {
                let mut plan = Plan::from(new);
                plan.set_default_availability(self.default_schedule.clone());
                plan.apply_update(PlanUpdate::User(Crud::Add(NewPlanUser { id: actor })))?;
                self.insert_plan(plan)
            }
            PlanTenantUpdate::Remove(id) => {
                self.authorize_plan(actor, is_admin, &id)?;
                self.plan_calendars.remove(&id);
                self.users
                    .values_mut()
                    .for_each(|u| u.favourites.retain(|p| *p != id));
                self.plans
                    .remove(&id)
                    .map(drop)
                    .ok_or(PlinkoError::PlanDoesNotExist(id))
            }
            PlanTenantUpdate::Update(id, update) => {
                self.authorize_plan(actor, is_admin, &id)?;
                self.ensure_plan_refs_exist(&update)?;
                let mut update = *update;
                self.set_status_dates(&id, &mut update);
                self.check_ranges(&update)?;
                let changes_availability = matches!(update, PlanUpdate::Availability(_));
                self.plans
                    .get_mut(&id)
                    .ok_or(PlinkoError::PlanDoesNotExist(id))?
                    .apply_update(update)?;
                if changes_availability {
                    self.plan_calendars.remove(&id);
                }
                Ok(())
            }
        }
    }

    /// A task that has started begins today unless it has a start. Completing, dropping or putting
    /// it on hold ends it today, unless the update brings a new end date, and an update that keeps
    /// the status keeps the end. A task in progress has no end, so resuming clears it. A task put
    /// back to not started has not run, so it has neither, and starting it again begins it today.
    /// The client sends back the dates it has, so an end the task already had is not a new one.
    fn set_status_dates(&self, plan: &ShortId, update: &mut PlanUpdate) {
        let PlanUpdate::Task(Crud::Update(patch)) = update else {
            return;
        };
        let was = self.plans.get(plan).and_then(|p| p.tasks().get(&patch.id));
        let Some(was) = was.map(|t| t.persistent_state()) else {
            return;
        };
        let Some(state) = &mut patch.persistent_state else {
            return;
        };
        if state.status == Status::NotStarted {
            state.actual_start_date = None;
            state.actual_end_date = None;
            return;
        }
        let start = state.actual_start_date.or(was.actual_start_date);
        state.actual_start_date = start.or(Some(self.today));
        let same = state.status == was.status;
        let given = state
            .actual_end_date
            .filter(|end| same || Some(*end) != was.actual_end_date);
        state.actual_end_date = match state.status {
            Status::InProgress => None,
            _ if same => given.or(was.actual_end_date).or(Some(self.today)),
            _ => given.or(Some(self.today)),
        };
    }

    /// Dates must be within `MAX_DAYS_FROM_TODAY` of today and durations at most `MAX_DURATION`,
    /// which keeps the days the scheduler looks at a sane number, and a task cannot end before
    /// it starts
    fn check_ranges(&self, update: &PlanUpdate) -> PlinkoResult<()> {
        let constraint = |c: Option<TimeConstraint>| {
            let (from, until) = c.map(TimeConstraint::bounds).unwrap_or_default();
            from.into_iter().chain(until)
        };
        let (dates, durations): (Vec<NaiveDate>, Vec<u32>) = match update {
            PlanUpdate::Task(Crud::Update(p)) => {
                let state = p.persistent_state.unwrap_or_default();
                let (start, end) = (state.actual_start_date, state.actual_end_date);
                if start.zip(end).is_some_and(|(start, end)| end < start) {
                    let why = "a task cannot end before it starts".to_string();
                    return Err(PlinkoError::OutOfRange(why));
                }
                let constrained = constraint(p.time_constraint.map(|c| c.constraint));
                let dates = constrained.chain(start).chain(end).collect();
                (dates, p.duration.into_iter().collect())
            }
            PlanUpdate::TaskGroup(Crud::Update(p)) => (
                constraint(p.time_constraint.map(|c| c.constraint)).collect(),
                Vec::new(),
            ),
            PlanUpdate::Milestone(Crud::Update(p)) => {
                let actual = p.persistent_state.as_ref().and_then(|s| s.actual_date);
                (
                    constraint(p.time_constraint).chain(actual).collect(),
                    Vec::new(),
                )
            }
            PlanUpdate::PlanBuffer(Crud::Add(new)) => (Vec::new(), vec![new.duration]),
            PlanUpdate::PlanBuffer(Crud::Update(p)) => (
                p.set_start_day.flatten().into_iter().collect(),
                p.duration.into_iter().collect(),
            ),
            PlanUpdate::PlanDelay(Crud::Add(new)) => (Vec::new(), vec![new.duration]),
            PlanUpdate::PlanDelay(Crud::Update(p)) => {
                (Vec::new(), p.duration.into_iter().collect())
            }
            _ => (Vec::new(), Vec::new()),
        };
        let far = |date: &NaiveDate| (*date - self.today).num_days().abs() > MAX_DAYS_FROM_TODAY;
        if let Some(date) = dates.iter().find(|d| far(d)) {
            let why = format!("{date} is more than {MAX_DAYS_FROM_TODAY} days from today");
            return Err(PlinkoError::OutOfRange(why));
        }
        if let Some(days) = durations.iter().find(|d| **d > MAX_DURATION) {
            let why = format!("{days} days is longer than the {MAX_DURATION} a duration may be");
            return Err(PlinkoError::OutOfRange(why));
        }
        Ok(())
    }

    /// Admins and anyone in the plan, directly or through an assigned group, may change a plan
    fn authorize_plan(&self, actor: Uuid, is_admin: bool, id: &ShortId) -> PlinkoResult<()> {
        let plan = self
            .plans
            .get(id)
            .ok_or(PlinkoError::PlanDoesNotExist(*id))?;
        allow(is_admin || self.in_plan(plan, &actor), actor)
    }

    fn in_plan(&self, plan: &Plan, user: &Uuid) -> bool {
        plan.users().contains_key(user)
            || plan
                .user_groups()
                .keys()
                .any(|g| self.group_members(*g).contains(user))
    }

    /// The plans a user may see and change: all of them for an admin, otherwise the ones they
    /// are in
    pub fn visible_plans(&self, user: &Uuid) -> HashSet<ShortId> {
        let is_admin = self
            .users
            .get(user)
            .is_some_and(|u| u.role() == Role::Admin);
        let plans = self.plans.iter();
        let visible = plans.filter(|(_, p)| is_admin || self.in_plan(p, user));
        visible.map(|(id, _)| *id).collect()
    }

    /// The tenant as a user may see it: everyone and every group, and their plans, with only
    /// those plans among anyone's favourites. It has no schedule, which a client is sent
    /// separately.
    pub fn visible_to(&self, user: &Uuid) -> Tenant {
        let visible = self.visible_plans(user);
        let mut users = self.users.clone();
        for user in users.values_mut() {
            user.favourites.retain(|p| visible.contains(p));
        }
        Tenant {
            id: self.id,
            name: self.name.clone(),
            today: self.today,
            users,
            user_groups: self.user_groups.clone(),
            volatile: HashMap::new(),
            plan_calendars: HashMap::new(),
            schedule: None,
            hints: Hints::default(),
            plans: self
                .plans
                .iter()
                .filter(|(id, _)| visible.contains(id))
                .map(|(id, plan)| (*id, plan.clone()))
                .collect(),
            default_schedule: self.default_schedule.clone(),
        }
    }

    /// Plans only know ids, so users and groups added to one must exist in the tenant
    fn ensure_plan_refs_exist(&self, update: &PlanUpdate) -> PlinkoResult<()> {
        let missing = match update {
            PlanUpdate::User(Crud::Add(new)) => {
                Some(new.id).filter(|id| !self.users.contains_key(id))
            }
            PlanUpdate::UserGroup(Crud::Add(new)) => {
                Some(new.id).filter(|id| !self.user_groups.contains_key(id))
            }
            PlanUpdate::Task(Crud::Update(patch)) => {
                let recorded = patch.actual_workers.iter().flat_map(|w| w.values());
                recorded
                    .filter_map(|who| match who {
                        Assignee::User(id) if !self.users.contains_key(id) => Some(*id),
                        _ => None,
                    })
                    .next()
            }
            _ => None,
        };
        missing.map_or(Ok(()), |id| Err(PlinkoError::ItemDoesNotExist(id)))
    }

    /// Turns what a client asked for into the update to log and apply: new items get fresh ids
    /// and the defaults from the last schedule are filled in, see `fill_defaults`
    pub fn stage(
        &self,
        time: DateTime<Utc>,
        actor: Uuid,
        update_type: TenantUpdateType,
    ) -> TenantUpdate {
        self.stage_from(self.schedule.as_ref(), time, actor, update_type)
    }

    /// `stage` with a schedule that is not the stored one, for a server whose schedule trails
    /// its updates
    pub fn stage_from(
        &self,
        schedule: Option<&TenantVolatileState>,
        time: DateTime<Utc>,
        actor: Uuid,
        mut update_type: TenantUpdateType,
    ) -> TenantUpdate {
        update_type.assign_ids();
        self.fill_defaults_from(schedule, &mut update_type);
        TenantUpdate {
            time,
            actor,
            update_type,
        }
    }

    /// The updates to make after `update` to drop the dependencies it made redundant. Applying
    /// them leaves nothing more to do.
    pub fn simplifications(&self, update: &TenantUpdateType) -> Vec<TenantUpdateType> {
        let TenantUpdateType::Plan(PlanTenantUpdate::Update(id, inner)) = update else {
            return Vec::new();
        };
        let Some(plan) = self.plans.get(id) else {
            return Vec::new();
        };
        plan.simplifications(inner)
            .into_iter()
            .map(|u| TenantUpdateType::Plan(PlanTenantUpdate::Update(*id, Box::new(u))))
            .collect()
    }

    /// Applies an update then schedules again, so `schedule` is up to date. Nothing changes if
    /// the update is refused.
    pub fn apply_and_schedule(&mut self, u: TenantUpdate) -> PlinkoResult<()> {
        self.apply_update(u)?;
        scheduler::schedule(self);
        Ok(())
    }

    /// Fills in what a task update leaves out that comes from the last schedule, so it is part of
    /// the update that is logged (applying an update cannot depend on volatile state). When a task
    /// goes in progress, on hold or completed with no one recorded, the people last scheduled
    /// for it are recorded. `stage` does it with `TenantUpdateType::assign_ids`.
    pub fn fill_defaults(&self, update: &mut TenantUpdateType) {
        self.fill_defaults_from(self.schedule.as_ref(), update);
    }

    pub fn fill_defaults_from(
        &self,
        schedule: Option<&TenantVolatileState>,
        update: &mut TenantUpdateType,
    ) {
        let TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, inner)) = update else {
            return;
        };
        let PlanUpdate::Task(Crud::Update(patch)) = &mut **inner else {
            return;
        };
        let started = patch.persistent_state.is_some_and(|s| {
            matches!(
                s.status,
                Status::InProgress | Status::OnHold | Status::Completed
            )
        });
        let task = self.plans.get(plan).and_then(|p| p.tasks().get(&patch.id));
        let unrecorded = task.is_some_and(|t| t.actual_workers().is_empty());
        if !started || !unrecorded || patch.actual_workers.is_some() {
            return;
        }
        let scheduled = schedule
            .and_then(|s| s.plans.get(plan))
            .and_then(|p| p.tasks.get(&patch.id));
        if let Some(scheduled) = scheduled.filter(|s| !s.workers.is_empty()) {
            patch.actual_workers = Some(scheduled.workers.clone());
        }
    }

    pub(super) fn admins(&self) -> impl Iterator<Item = &Uuid> {
        self.users
            .iter()
            .filter(|(_, u)| u.role() == Role::Admin)
            .map(|(id, _)| id)
    }

    /// A user's availability calendar, as of the last `refresh_calendars`
    pub fn calendar(&self, user: &Uuid) -> Option<&Calendar<Workload>> {
        self.volatile.get(user)?.calendar.as_ref()
    }

    /// A plan's default availability calendar, as of the last `refresh_calendars`
    pub fn plan_calendar(&self, plan: &ShortId) -> Option<&Calendar<Workload>> {
        self.plan_calendars.get(plan)
    }

    /// Makes sure every user and plan default has a calendar covering `from..=until`, rebuilding
    /// only those whose rules changed since or that do not cover the range
    pub fn refresh_calendars(&mut self, from: NaiveDate, until: NaiveDate) {
        for (id, user) in &self.users {
            let state = self.volatile.entry(*id).or_default();
            if !state
                .calendar
                .as_ref()
                .is_some_and(|c| c.covers(from, until))
            {
                state.calendar = Some(user.availability().calendar(from, until));
            }
        }
        for (id, plan) in &self.plans {
            let calendar = self.plan_calendars.get(id);
            if !calendar.is_some_and(|c| c.covers(from, until)) {
                let calendar = plan.default_availability().calendar(from, until);
                self.plan_calendars.insert(*id, calendar);
            }
        }
    }

    /// Everyone in the group or any group below it
    pub fn group_members(&self, group: Uuid) -> HashSet<Uuid> {
        let mut members = HashSet::new();
        let mut pending = vec![group];
        while let Some(id) = pending.pop() {
            if let Some(group) = self.user_groups.get(&id) {
                members.extend(group.users());
                let children = self
                    .user_groups
                    .iter()
                    .filter(|(_, g)| g.parent() == Some(id));
                pending.extend(children.map(|(child, _)| *child));
            }
        }
        members
    }
}
// }}}

// allow {{{
fn allow(permitted: bool, actor: Uuid) -> PlinkoResult<()> {
    permitted.then_some(()).ok_or(PlinkoError::Forbidden(actor))
}
// }}}
