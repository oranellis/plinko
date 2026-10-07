// modules{{{
use crate::{multitenancy::tenant::Tenant, scheduler::state::TenantVolatileState};
// }}}

pub mod state;

/// Schedules every plan in the tenant together, since people work across plans
pub fn solve_tenant(_tenant: &Tenant) -> TenantVolatileState {
    todo!()
}
