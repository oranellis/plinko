use std::collections::HashMap;

use uuid::Uuid;

use crate::{multitenancy::user::TenantLoginUser, planner::plan::Plan, types::short_id::ShortId};

pub struct Tenant {
    id: ShortId,
    name: String,
    users: HashMap<Uuid, TenantLoginUser>,
    plans: HashMap<ShortId, Plan>,
}

impl Tenant {
    pub fn new(id: ShortId, name: String) -> Self {
        let mut users = HashMap::new();
        let root_user = TenantLoginUser::new("Root User".to_string(), "root@localhost".to_string());
        users.insert(*root_user.id(), root_user);
        Self {
            id,
            name,
            users,
            plans: HashMap::new(),
        }
    }

    pub fn new_with_login(id: ShortId, name: String, user: TenantLoginUser) -> Self {
        let mut users = HashMap::new();
        users.insert(*user.id(), user);
        Self {
            id,
            name,
            users: HashMap::new(),
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

    pub fn plans(&self) -> &HashMap<ShortId, Plan> {
        &self.plans
    }
}
