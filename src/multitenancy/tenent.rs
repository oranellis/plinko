use std::collections::HashMap;

use crate::multitenancy::user::{TenantUser, TenantUserUuid};

struct Tenant {
    users: HashMap<TenantUserUuid, TenantUser>,
}
