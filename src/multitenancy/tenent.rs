use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    multitenancy::user::TenantUser,
    planner::plan::{Plan, ShortId},
};

struct Tenant {
    users: HashMap<Uuid, TenantUser>,
    plans: HashMap<ShortId, Plan>,
}
