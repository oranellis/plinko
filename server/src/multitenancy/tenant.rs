// modules{{{
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use uuid::Uuid;

use crate::{
    availability::calendar::Calendar,
    multitenancy::{
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
        user::{Role, TenantLoginUser, TenantUserVolatileState},
        user_group::TenantUserGroup,
    },
    planner::{plan::Plan, plan_update::PlanUpdate},
    scheduler::state::TenantVolatileState,
    traits::entity::Entity,
    types::{
        crud::Crud,
        error::{PlinkoError, PlinkoResult},
        short_id::ShortId,
        task::{Assignee, Status},
        user::NewPlanUser,
        workload::Workload,
    },
};
// }}}

pub struct Tenant {
    id: ShortId,
    name: String,
    pub(super) users: HashMap<Uuid, TenantLoginUser>,
    pub(super) user_groups: HashMap<Uuid, TenantUserGroup>,
    volatile: HashMap<Uuid, TenantUserVolatileState>,
    plan_calendars: HashMap<ShortId, Calendar<Workload>>,
    schedule: Option<TenantVolatileState>,
    pub(super) plans: HashMap<ShortId, Plan>,
}

// impl Tenant{{{
impl Tenant {
    pub fn new(id: ShortId, name: String) -> Self {
        let root_user = TenantLoginUser::new(
            "Root User".to_string(),
            "root@localhost".to_string(),
            Role::Admin,
        );
        Self::new_with_login(id, name, root_user)
    }

    /// Create a new tenant with a specific initial login
    ///
    /// * `id`: A unique ID for the tenant
    /// * `name`: A friendly name for the tenant
    /// * `user`: An initial user to populate the tenant with
    pub fn new_with_login(id: ShortId, name: String, user: TenantLoginUser) -> Self {
        let mut users = HashMap::new();
        users.insert(*user.id(), user);
        Self {
            id,
            name,
            users,
            user_groups: HashMap::new(),
            volatile: HashMap::new(),
            plan_calendars: HashMap::new(),
            schedule: None,
            plans: HashMap::new(),
        }
    }

    pub fn id(&self) -> &ShortId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
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
                let removed = match &update {
                    Crud::Remove(id) => Some(*id),
                    _ => None,
                };
                TenantLoginUser::apply_crud(self, update)?;
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
                plan.apply_update(PlanUpdate::User(Crud::Add(NewPlanUser { id: actor })))?;
                self.insert_plan(plan)
            }
            PlanTenantUpdate::Remove(id) => {
                self.authorize_plan(actor, is_admin, &id)?;
                self.plan_calendars.remove(&id);
                self.plans
                    .remove(&id)
                    .map(drop)
                    .ok_or(PlinkoError::PlanDoesNotExist(id))
            }
            PlanTenantUpdate::Update(id, update) => {
                self.authorize_plan(actor, is_admin, &id)?;
                self.ensure_plan_refs_exist(&update)?;
                let update = *update;
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

    /// Admins and anyone in the plan, directly or through an assigned group, may change a plan
    fn authorize_plan(&self, actor: Uuid, is_admin: bool, id: &ShortId) -> PlinkoResult<()> {
        let plan = self
            .plans
            .get(id)
            .ok_or(PlinkoError::PlanDoesNotExist(*id))?;
        let in_plan = plan.users().contains_key(&actor)
            || plan
                .user_groups()
                .keys()
                .any(|g| self.group_members(*g).contains(&actor));
        allow(is_admin || in_plan, actor)
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

    /// Fills in what a task update leaves out that comes from the last schedule, so it is part of
    /// the update that is logged (applying an update cannot depend on volatile state). When a task
    /// goes in progress, on hold or completed with no one recorded, the people last scheduled
    /// for it are recorded. A server handler calls this next to `TenantUpdateType::assign_ids`.
    pub fn fill_defaults(&self, update: &mut TenantUpdateType) {
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
        let scheduled = self
            .schedule
            .as_ref()
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

fn allow(permitted: bool, actor: Uuid) -> PlinkoResult<()> {
    permitted.then_some(()).ok_or(PlinkoError::Forbidden(actor))
}
