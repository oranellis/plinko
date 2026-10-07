//! The messages on the WebSocket, in the format of `codec`. A client sends updates and the server
//! answers each, and sends everything that changes what that client may see.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    multitenancy::{
        tenant::Tenant,
        tenant_update::{TenantUpdate, TenantUpdateType},
    },
    scheduler::state::TenantVolatileState,
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
    /// The client's update was applied as update `seq`
    Ack {
        request: u32,
        seq: u64,
    },
    Rejected {
        request: u32,
        error: String,
    },
}
