//! The messages on the WebSocket, in the format of `codec`. A client sends updates and the server
//! answers each, and sends everything that changes what that client may see.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{PlanTenantUpdate, TenantUpdate, TenantUpdateType},
    },
    planner::plan_update::PlanUpdate,
    scheduler::state::TenantVolatileState,
    types::{crud::Crud, short_id::ShortId},
};

#[derive(Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// A change to make as the logged in user. `request` comes back in the answer.
    Update {
        request: u32,
        update: TenantUpdateType,
    },
}

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    /// The tenant as `user` may see it after update `seq`, replacing what the client has. Sent
    /// on connecting, and when what the user may see changes.
    Snapshot {
        seq: u64,
        user: Uuid,
        tenant: Box<Tenant>,
    },
    /// Update `seq`, to apply to the tenant the client has
    Update {
        seq: u64,
        update: TenantUpdate,
    },
    /// The schedule that results from the updates up to `seq`, which can trail the latest update
    /// while it is worked out
    Schedule {
        seq: u64,
        schedule: Box<TenantVolatileState>,
    },
    /// The client's update was applied as update `seq`, making `created` if it added something
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

/// Who is logged in, from `/api/login` and `/api/me`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub admin: bool,
}

/// What an update made, so a client can go straight to it
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Created {
    Plan(ShortId),
    Item(Uuid),
}

impl Created {
    /// What the update adds, if it adds a plan or an item of one
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
                _ => None,
            },
            _ => None,
        }
    }
}
