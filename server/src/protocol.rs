//! WebSocket messages, encoded with `codec`

// modules {{{
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    common::{crud::Crud, short_id::ShortId},
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
    },
    planner::plan_update::PlanUpdate,
    scheduler::state::TenantVolatileState,
};
// }}}

#[derive(Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// `request` comes back in the answer
    Update {
        request: u32,
        update: TenantUpdateType,
    },
}

// ClientMessage supporting types {{{
#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    /// The tenant as `user` may see it, replacing what the client has. Sent on connecting and
    /// whenever what the user may see changes.
    Snapshot {
        seq: u64,
        user: Uuid,
        tenant: Box<Tenant>,
    },
    Update {
        seq: u64,
        update: TenantUpdate,
    },
    /// `seq` can trail the latest update while the schedule is worked out
    Schedule {
        seq: u64,
        schedule: Box<TenantVolatileState>,
    },
    Ack {
        request: u32,
        seq: u64,
        created: Option<Created>,
    },
    Rejected {
        request: u32,
        error: String,
    },
}

/// From `/api/login` and `/api/me`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub admin: bool,
}

/// So a client can go straight to what its update made
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Created {
    Plan(ShortId),
    Item(Uuid),
}
// }}}

// impl Created {{{
impl Created {
    pub fn by(update: &TenantUpdateType) -> Option<Self> {
        match update {
            TenantUpdateType::User(Crud::Add(new)) => Some(Created::Item(new.id)),
            TenantUpdateType::UserGroup(Crud::Add(new)) => Some(Created::Item(new.id)),
            TenantUpdateType::Plan(PlanTenantUpdate::Add(new)) => Some(Created::Plan(new.id)),
            TenantUpdateType::Plan(PlanTenantUpdate::Update(_, inner)) => match &**inner {
                PlanUpdate::Task(Crud::Add(new)) => Some(Created::Item(new.id)),
                PlanUpdate::Milestone(Crud::Add(new)) => Some(Created::Item(new.id)),
                PlanUpdate::TaskGroup(Crud::Add(new)) => Some(Created::Item(new.id)),
                PlanUpdate::PlanBuffer(Crud::Add(new)) => Some(Created::Item(new.id)),
                PlanUpdate::PlanDelay(Crud::Add(new)) => Some(Created::Item(new.id)),
                _ => None,
            },
            _ => None,
        }
    }
}
// }}}
