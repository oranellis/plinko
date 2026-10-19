//! What an update can change in the schedule, so the schedule can be brought up to date without
//! working everything out again: nothing, some nodes of a plan, one whole plan, or everything.

// modules {{{
use std::collections::{HashMap, HashSet};

use crate::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdateType},
    },
    planner::plan_update::PlanUpdate,
    types::{crud::Crud, dependency::PlanNode, short_id::ShortId},
};
// }}}

/// What an update changes in one plan
#[derive(Clone, Debug, PartialEq)]
pub enum PlanChange {
    /// These nodes and so what depends on them, the rest of the plan stays where it is
    Nodes(HashSet<PlanNode>),
    /// Anything in the plan, which is scheduled again among the rest
    Whole,
}

// PlanChange supporting types {{{
/// What a run of updates changes
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Change {
    /// Everything has to be worked out again
    pub everything: bool,
    pub plans: HashMap<ShortId, PlanChange>,
    /// Plans that are gone, whose hours are free again
    pub removed: HashSet<ShortId>,
}
// }}}

// impl Change {{{
impl Change {
    /// Whether the schedule is still right
    pub fn is_none(&self) -> bool {
        !self.everything && self.plans.is_empty() && self.removed.is_empty()
    }

    fn nodes(&mut self, plan: ShortId, nodes: impl IntoIterator<Item = PlanNode>) {
        match self
            .plans
            .entry(plan)
            .or_insert(PlanChange::Nodes(HashSet::new()))
        {
            PlanChange::Nodes(set) => set.extend(nodes),
            PlanChange::Whole => {}
        }
    }

    fn whole(&mut self, plan: ShortId) {
        self.plans.insert(plan, PlanChange::Whole);
    }

    /// Notes what an update about to be applied to `tenant` may change
    pub fn note(&mut self, tenant: &Tenant, update: &TenantUpdateType) {
        match update {
            // a new user has no work until a plan has them, and a profile is not scheduled
            TenantUpdateType::User(Crud::Add(_) | Crud::Update(_)) => {}
            TenantUpdateType::User(Crud::Remove(_)) => self.everything = true,
            TenantUpdateType::UserGroup(Crud::Add(_)) => {}
            TenantUpdateType::UserGroup(_) => self.everything = true,
            TenantUpdateType::Availability(..) => self.everything = true,
            TenantUpdateType::SetToday(day) => self.everything |= *day != tenant.today(),
            TenantUpdateType::Favourite(..) => {}
            TenantUpdateType::Plan(PlanTenantUpdate::Add(new)) => self.whole(new.id),
            TenantUpdateType::Plan(PlanTenantUpdate::Remove(id)) => {
                self.plans.remove(id);
                self.removed.insert(*id);
            }
            TenantUpdateType::Plan(PlanTenantUpdate::Update(plan, update)) => {
                self.plan_update(*plan, update);
            }
        }
    }

    fn plan_update(&mut self, plan: ShortId, update: &PlanUpdate) {
        match update {
            PlanUpdate::Task(Crud::Add(new)) => self.nodes(plan, [PlanNode::Task(new.id)]),
            PlanUpdate::Task(Crud::Update(p)) => {
                let moves = p.parent.is_some()
                    || p.mode.is_some()
                    || p.duration.is_some()
                    || p.time_constraint.is_some()
                    || p.dependencies.is_some()
                    || p.allocations.is_some()
                    || p.affinities.is_some()
                    || p.actual_workers.is_some()
                    || p.persistent_state.is_some();
                if moves {
                    self.nodes(plan, [PlanNode::Task(p.id)]);
                }
            }
            PlanUpdate::Milestone(Crud::Add(new)) => {
                self.nodes(plan, [PlanNode::Milestone(new.id)]);
            }
            PlanUpdate::Milestone(Crud::Update(p)) => {
                if p.parent.is_some()
                    || p.time_constraint.is_some()
                    || p.dependencies.is_some()
                    || p.persistent_state.is_some()
                {
                    self.nodes(plan, [PlanNode::Milestone(p.id)]);
                }
            }
            PlanUpdate::TaskGroup(Crud::Add(new)) => {
                self.nodes(plan, [PlanNode::TaskGroup(new.id)]);
            }
            PlanUpdate::TaskGroup(Crud::Update(p)) => {
                if p.parent.is_some() || p.dependencies.is_some() {
                    self.nodes(plan, [PlanNode::TaskGroup(p.id)]);
                }
            }
            PlanUpdate::PlanBuffer(Crud::Add(new)) => {
                self.nodes(plan, [PlanNode::PlanBuffer(new.id)]);
            }
            PlanUpdate::PlanBuffer(Crud::Update(p)) => {
                if p.parent.is_some()
                    || p.dependencies.is_some()
                    || p.set_start_day.is_some()
                    || p.duration.is_some()
                {
                    self.nodes(plan, [PlanNode::PlanBuffer(p.id)]);
                }
            }
            // what was removed leaves its hours, and what depended on it was rewired by updates
            // that noted their own nodes
            PlanUpdate::Task(Crud::Remove(_))
            | PlanUpdate::Milestone(Crud::Remove(_))
            | PlanUpdate::TaskGroup(Crud::Remove(_))
            | PlanUpdate::PlanBuffer(Crud::Remove(_)) => {
                self.nodes(plan, []);
            }
            // words and notes are not scheduled
            PlanUpdate::Data(..) | PlanUpdate::Describe(..) => {}
            PlanUpdate::Tag(Crud::Update(_)) | PlanUpdate::Affinity(Crud::Update(_)) => {}
            // who can do what, when, and what comes first all change much of a plan
            PlanUpdate::Tag(_)
            | PlanUpdate::Affinity(_)
            | PlanUpdate::AffinityRules(..)
            | PlanUpdate::User(_)
            | PlanUpdate::UserGroup(_)
            | PlanUpdate::Target(_)
            | PlanUpdate::Availability(_)
            | PlanUpdate::SchedulerData(_) => self.whole(plan),
        }
    }
}
// }}}
